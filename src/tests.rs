use super::{Code, Data, Vtable};
use bincode;
use metatype;
use serde_derive::{Deserialize, Serialize};
use serde_json;
use std::{any, env, fmt, mem, process, str};

#[test]
fn multi_process() {
	#[derive(Serialize, Deserialize)]
	#[serde(bound(serialize = ""), bound(deserialize = ""))]
	struct Xxx<A: 'static, B: 'static + ?Sized> {
		a: Data<[u8; 5]>,
		b: Code<()>,
		c: Vtable<()>,
		d: Code<A>,
		e: Vtable<B>,
	}
	impl<A: 'static, B: 'static + ?Sized> PartialEq for Xxx<A, B> {
		#[inline(always)]
		fn eq(&self, other: &Self) -> bool {
			self.a == other.a
				&& self.b == other.b
				&& self.c == other.c
				&& self.d == other.d
				&& self.e == other.e
		}
	}
	impl<A: 'static, B: 'static + ?Sized> fmt::Debug for Xxx<A, B> {
		fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
			f.debug_struct("Xxx")
				.field("a", &self.a)
				.field("b", &self.b)
				.field("c", &self.c)
				.field("d", &self.d)
				.field("e", &self.e)
				.finish()
		}
	}
	unsafe fn code<T>(_: &T, ptr: *const ()) -> Code<T> {
		Code::from(ptr)
	}
	unsafe fn vtable<T: ?Sized>(_: &T, ptr: &'static ()) -> Vtable<T> {
		Vtable::from(ptr)
	}
	fn eq<T: ?Sized>(_: &T, _: &T) {}
	let trait_object: Box<dyn any::Any> = Box::new(1234_usize);
	let meta: metatype::TraitObject =
		unsafe { mem::transmute_copy(&<dyn any::Any as metatype::Type>::meta(&*trait_object)) };
	let a = Xxx {
		a: unsafe { Data::from(&[0, 1, 2, 3, 4]) },
		b: unsafe { Code::from(multi_process as *const ()) },
		c: unsafe { Vtable::from(meta.vtable) },
		d: unsafe { code(&multi_process, multi_process as *const ()) },
		e: unsafe { vtable(&*trait_object, meta.vtable) },
	};
	let exe = env::current_exe().unwrap();
	if let Ok(x) = env::var("SPAWNED_TOKEN_RELATIVE") {
		let (a2, bc): (_, Vec<u8>) = serde_json::from_str(&x).unwrap();
		eq(&a, &a2);
		let a3 = bincode::deserialize(&bc).unwrap();
		eq(&a, &a3);
		assert_eq!(a, a2);
		assert_eq!(a, a3);
		println!("success_token_relative {:?}", a2);
		return;
	}
	for i in 0..100 {
		let output = process::Command::new(&exe)
			.arg("--nocapture")
			.arg("--exact")
			.arg("tests::multi_process")
			.env(
				"SPAWNED_TOKEN_RELATIVE",
				serde_json::to_string(&(&a, bincode::serialize(&a).unwrap())).unwrap(),
			)
			.output()
			.unwrap();
		if !str::from_utf8(&output.stdout)
			.unwrap()
			.contains("success_token_relative")
			|| !output.status.success()
		{
			panic!("{}: {:?}", i, output);
		}
	}
}
