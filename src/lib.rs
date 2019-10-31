//! A type to wrap vtable references such that they can be safely sent
//! between other processes running the same binary.
//!
//! **[Crates.io](https://crates.io/crates/relative) │ [Repo](https://github.com/alecmocatta/relative)**
//!
//! References are adjusted relative to a base when (de)serialised, which
//! is what enables it to work across binaries that are dynamically loaded at
//! different addresses under multiple invocations.
//!
//! It being the same binary is checked by serialising the
//! [`build_id`](https://docs.rs/build_id) alongside the relative pointer, which
//! is validated at deserialisation.
//!
//! # Example
//! ### Local process
//! ```
//! # #![feature(raw)]
//! # use relative::*;
//! use std::{fmt::Display, mem::transmute, raw::TraitObject};
//!
//! let mut x: Box<dyn Display> = Box::new("hello world");
//! let x_ptr: *mut dyn Display = &mut *x;
//! let x_ptr: TraitObject = unsafe { transmute(x_ptr) };
//! let relative = unsafe { Vtable::<dyn Display>::from(&*x_ptr.vtable) };
//! // send `relative` to remote...
//! ```
//! ### Remote process
//! ```
//! # #![feature(raw)]
//! # use relative::*;
//! # use std::{fmt::Display, mem::transmute, raw::TraitObject};
//! # let mut x: Box<dyn Display> = Box::new("hello world");
//! # let x_ptr: *mut dyn Display = &mut *x;
//! # let x_ptr: TraitObject = unsafe { transmute(x_ptr) };
//! # let relative = unsafe { Vtable::<dyn Display>::from(&*x_ptr.vtable) };
//! // receive `relative`
//! let x: Box<&str> = Box::new("goodbye world");
//! let x_ptr = Box::into_raw(x);
//! let y_ptr = TraitObject { data: x_ptr.cast(), vtable: relative.to() as *const () as *mut () };
//! let y_ptr: *mut dyn Display = unsafe { transmute(y_ptr) };
//! let y: Box<dyn Display> = unsafe { Box::from_raw(y_ptr) };
//! println!("{}", y);
//! // prints "goodbye world"
//! ```

#![doc(html_root_url = "https://docs.rs/relative/0.2.0")]
#![cfg_attr(feature = "nightly", feature(raw))]
#![warn(
	missing_copy_implementations,
	missing_debug_implementations,
	missing_docs,
	trivial_casts,
	trivial_numeric_casts,
	unused_import_braces,
	unused_qualifications,
	unused_results,
	clippy::pedantic
)] // from https://github.com/rust-unofficial/patterns/blob/master/anti_patterns/deny-warnings.md
#![allow(
	clippy::inline_always,
	clippy::trivially_copy_pass_by_ref,
	clippy::must_use_candidate
)]

use serde::{
	de::{self, Deserialize, Deserializer}, ser::{Serialize, Serializer}
};
use std::{
	any::{type_name, Any, TypeId}, cmp, fmt, hash, marker, mem::transmute
};
use uuid::Uuid;

#[doc(hidden)]
#[used]
#[no_mangle]
pub static RELATIVE_VTABLE_BASE: &(dyn Any + Sync) = &();

fn type_id<T: ?Sized + 'static>() -> u64 {
	use std::hash::{Hash, Hasher};
	let type_id = TypeId::of::<T>();
	let mut hasher = std::collections::hash_map::DefaultHasher::new();
	type_id.hash(&mut hasher);
	hasher.finish()
}

/// This is obviously a terrible no good hack to avoid requiring nightly.
/// As well as the static size guarantee, it's correctness is asserted with the
/// "nightly" feature, which should provide adequate warning in the event that
/// this changes. Trait object layout is pretty baked into the compiler so such
/// a change is unlikely to happen suddenly/silently.
#[repr(C)]
#[derive(Copy, Clone)]
#[allow(missing_debug_implementations, missing_docs)]
struct TraitObject {
	data: *mut (),
	vtable: *mut (),
}

/// Wraps `&'static` references to vtables such that they can be safely sent
/// between other processes running the same binary.
///
/// For references into the segment that houses the vtables, typically the
/// read-only data segment aka rodata.
///
/// The base used is the vtable of a static trait object:
/// ```ignore
/// #[used]
/// #[no_mangle]
/// pub static RELATIVE_VTABLE_BASE: &(dyn Any + Sync) = &();
///
/// let base = transmute::<*const dyn Any, std::raw::TraitObject>(RELATIVE_VTABLE_BASE).vtable as usize;
/// ```
pub struct Vtable<T: ?Sized>(usize, marker::PhantomData<fn(T)>);
impl<T: ?Sized> Vtable<T> {
	#[inline(always)]
	fn new(p: usize) -> Self {
		Self(p, marker::PhantomData)
	}
	/// Create a `Vtable<T>` from a `&'static ()`.
	///
	/// # Safety
	///
	/// This is unsafe as it is up to the user to ensure the pointer lies within
	/// static memory.
	///
	/// i.e. the pointer needs to be positioned the same relative to the base in
	/// every invocation, through e.g. being in the same segment, or the binary
	/// being statically linked.
	#[inline(always)]
	pub unsafe fn from(ptr: &'static ()) -> Self {
		let base = transmute::<*const dyn Any, TraitObject>(RELATIVE_VTABLE_BASE).vtable as usize;
		#[cfg(feature = "nightly")]
		{
			let check_base =
				transmute::<*const dyn Any, std::raw::TraitObject>(RELATIVE_VTABLE_BASE).vtable
					as usize;
			assert_eq!(check_base, base);
		}
		Self::new(
			({
				let ptr: *const () = ptr;
				ptr
			} as usize)
				.wrapping_sub(base),
		)
	}
	/// Get back a `&'static ()` from a `Vtable<T>`.
	#[inline(always)]
	pub fn to(&self) -> &'static () {
		let base = unsafe { transmute::<*const dyn Any, TraitObject>(RELATIVE_VTABLE_BASE) }.vtable
			as usize;
		#[cfg(feature = "nightly")]
		{
			let check_base =
				unsafe { transmute::<*const dyn Any, std::raw::TraitObject>(RELATIVE_VTABLE_BASE) }
					.vtable as usize;
			assert_eq!(check_base, base);
		}
		unsafe { &*(base.wrapping_add(self.0) as *const ()) }
	}
}
impl<T: ?Sized> Clone for Vtable<T> {
	#[inline(always)]
	fn clone(&self) -> Self {
		Self(self.0, marker::PhantomData)
	}
}
impl<T: ?Sized> Copy for Vtable<T> {}
impl<T: ?Sized> PartialEq for Vtable<T> {
	#[inline(always)]
	fn eq(&self, other: &Self) -> bool {
		self.0 == other.0
	}
}
impl<T: ?Sized> Eq for Vtable<T> {}
impl<T: ?Sized> hash::Hash for Vtable<T> {
	#[inline(always)]
	fn hash<H: hash::Hasher>(&self, state: &mut H) {
		self.0.hash(state)
	}
}
impl<T: ?Sized> PartialOrd for Vtable<T> {
	#[inline(always)]
	fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
		self.0.partial_cmp(&other.0)
	}
}
impl<T: ?Sized> Ord for Vtable<T> {
	#[inline(always)]
	fn cmp(&self, other: &Self) -> cmp::Ordering {
		self.0.cmp(&other.0)
	}
}
impl<T: ?Sized> fmt::Debug for Vtable<T> {
	fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
		f.debug_struct("Vtable")
			.field(type_name::<T>(), &self.0)
			.finish()
	}
}
impl<T: ?Sized + 'static> Serialize for Vtable<T> {
	#[inline]
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		<(Uuid, u64, usize) as Serialize>::serialize(
			&(build_id::get(), type_id::<T>(), self.0),
			serializer,
		)
	}
}
impl<'de, T: ?Sized + 'static> Deserialize<'de> for Vtable<T> {
	#[inline]
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: Deserializer<'de>,
	{
		<(Uuid, u64, usize) as Deserialize<'de>>::deserialize(deserializer).and_then(
			|(build, id, ptr)| {
				let local = build_id::get();
				if build == local {
					if id == type_id::<T>() {
						Ok(Self::new(ptr))
					} else {
						Err(de::Error::custom(format_args!(
							"relative reference to wrong type ???:{}, expected {}:{}",
							id,
							type_name::<T>(),
							type_id::<T>()
						)))
					}
				} else {
					Err(de::Error::custom(format_args!(
						"relative reference came from a different binary {}, expected {}",
						build, local
					)))
				}
			},
		)
	}
}

#[cfg(test)]
mod tests {
	use super::{type_id, Vtable};
	use bincode;
	use metatype;
	use serde_derive::{Deserialize, Serialize};
	use serde_json;
	use std::{any::Any, env, fmt, process, str};

	#[test]
	fn type_id_sanity() {
		struct A;
		struct B;
		assert_ne!(type_id::<u8>(), type_id::<u16>());
		assert_ne!(type_id::<A>(), type_id::<B>());
		assert_eq!(type_id::<u8>(), type_id::<u8>());
		assert_eq!(type_id::<A>(), type_id::<A>());
	}

	#[test]
	fn multi_process() {
		#[derive(Serialize, Deserialize)]
		#[serde(bound(serialize = ""), bound(deserialize = ""))]
		struct Xxx<A: 'static + ?Sized> {
			a: Vtable<()>,
			b: Vtable<A>,
		}
		impl<A: 'static + ?Sized> PartialEq for Xxx<A> {
			#[inline(always)]
			fn eq(&self, other: &Self) -> bool {
				self.a == other.a && self.b == other.b
			}
		}
		impl<A: 'static + ?Sized> fmt::Debug for Xxx<A> {
			fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
				f.debug_struct("Xxx")
					.field("a", &self.a)
					.field("b", &self.b)
					.finish()
			}
		}
		unsafe fn vtable<T: ?Sized>(_: &T, ptr: &'static ()) -> Vtable<T> {
			Vtable::from(ptr)
		}
		fn eq<T: ?Sized>(_: &T, _: &T) {}
		let trait_object: Box<dyn Any> = Box::new(1234_usize);
		let meta: metatype::TraitObject =
			metatype::type_coerce(<dyn Any as metatype::Type>::meta(&*trait_object));
		let a = Xxx {
			a: unsafe { Vtable::from(meta.vtable) },
			b: unsafe { vtable(&*trait_object, meta.vtable) },
		};
		let bincoded = bincode::serialize(&a).unwrap();
		let jsoned = serde_json::to_string(&a).unwrap();
		let unbincoded = bincode::deserialize(&bincoded).unwrap();
		let unjsoned = serde_json::from_str(&jsoned).unwrap();
		eq(&a, &unbincoded);
		eq(&a, &unjsoned);
		assert_eq!(a, unbincoded);
		assert_eq!(a, unjsoned);

		if cfg!(not(miri)) {
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
			let exe = env::current_exe().unwrap();
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
	}
}
