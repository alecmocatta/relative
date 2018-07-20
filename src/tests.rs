use super::{Code, Pointer, Vtable};
use bincode;
use metatype;
use serde_json;
use std::{any, env, fmt, mem, process, str};
#[test]
fn multi_process() {
	#[derive(Serialize, Deserialize)]
	#[serde(bound(serialize = ""), bound(deserialize = ""))]
	struct Xxx<A: 'static, B: 'static + ?Sized> {
		a: Pointer<[u8; 5]>,
		b: Pointer<Code<()>>,
		c: Pointer<Vtable<()>>,
		d: Pointer<Code<A>>,
		e: Pointer<Vtable<B>>,
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
	fn code<T>(_: &T, ptr: *const ()) -> *const Code<T> {
		ptr as *const Code<T>
	}
	fn vtable<T: ?Sized>(_: &T, ptr: *const ()) -> *const Vtable<T> {
		ptr as *const Vtable<T>
	}
	fn eq<T: ?Sized>(_: &T, _: &T) {}
	let trait_object: Box<any::Any> = Box::new(1234usize);
	let meta: metatype::TraitObject =
		unsafe { mem::transmute_copy(&<any::Any as metatype::Type>::meta(&*trait_object)) };
	let a = Xxx {
		a: unsafe { Pointer::from(&[0, 1, 2, 3, 4]) },
		b: unsafe { Pointer::from(&*(&(multi_process as fn()) as *const fn() as *const Code<()>)) },
		c: unsafe {
			Pointer::from(mem::transmute::<&'static (), &'static Vtable<()>>(
				meta.vtable,
			))
		},
		d: unsafe { Pointer::from(&*code(&multi_process, multi_process as fn() as *const ())) },
		e: unsafe { Pointer::from(&*vtable(&*trait_object, meta.vtable)) },
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
			.contains("success_token_relative") || !output.status.success()
		{
			panic!("{}: {:?}", i, output);
		}
	}
}
