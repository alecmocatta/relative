//! A type to wrap `&'static` references such that they can be safely sent
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
//! # use relative::*;
//! let x: &'static [u16;4] = &[2,3,5,8];
//! // unsafe as it's up to the user to ensure the reference is into static memory
//! let relative = unsafe{Data::from(x)};
//! // send `relative` to remote...
//! ```
//! ### Remote process
//! ```
//! # use relative::*;
//! # let x: &'static [u16;4] = &[2,3,5,8];
//! # let relative = unsafe{Data::from(x)};
//! // receive `relative`
//! println!("{:?}", relative.to());
//! // prints "[2, 3, 5, 8]"
//! ```
//!
//! # Note
//!
//! This currently requires Rust nightly.

#![doc(html_root_url = "https://docs.rs/relative/0.1.4")]
#![cfg_attr(feature = "nightly", feature(core_intrinsics, raw))]
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
#![allow(clippy::inline_always, clippy::trivially_copy_pass_by_ref)]

use serde::{
	de::{self, Deserialize, Deserializer}, ser::{Serialize, Serializer}
};
use std::{
	any::{self, type_name}, cmp, fmt, hash, hint, marker, mem
};
use uuid::Uuid;

#[doc(hidden)]
#[used]
#[no_mangle]
pub static RELATIVE_DATA_BASE: () = ();

#[doc(hidden)]
#[no_mangle]
#[inline(never)]
pub fn relative_code_base() {
	unsafe { hint::unreachable_unchecked() }
}

#[doc(hidden)]
#[used]
#[no_mangle]
pub static RELATIVE_VTABLE_BASE: &(dyn any::Any + Sync) = &();

/// This is obviously a terrible no good hack to avoid requiring nightly.
/// As well as the static size guarantee, it's correctness is asserted with the
/// "nightly" feature, which should provide adequate warning in the event that
/// this changes. Even if there is a change it's hard to imagine how it could
/// cause invalid or unsound behaviour.
#[allow(clippy::let_and_return)]
fn type_id<T: ?Sized + 'static>() -> u64 {
	let ret = unsafe { mem::transmute(any::TypeId::of::<T>()) };
	#[cfg(feature = "nightly")]
	assert_eq!(unsafe { std::intrinsics::type_id::<T>() }, ret);
	ret
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

/// Wraps function pointers such that they can be safely sent between other
/// processes running the same binary.
///
/// For references into the code aka text segment.
///
/// The base used is the address of a function:
/// ```ignore
/// #[no_mangle]
/// #[inline(never)]
/// pub fn relative_code_base() {
/// 	unsafe { std::hint::unreachable_unchecked() }
/// }
///
/// let base = relative_code_base as usize;
/// ```
pub struct Code<T: ?Sized>(usize, marker::PhantomData<fn(T)>);
impl<T: ?Sized> Code<T> {
	#[inline(always)]
	fn new(p: usize) -> Self {
		Self(p, marker::PhantomData)
	}
	/// Create a `Code<T>` from a `*const ()`.
	///
	/// This is unsafe as it is up to the user to ensure the pointer lies within
	/// static memory.
	///
	/// i.e. the pointer needs to be positioned the same relative to the base in
	/// every invocation, through e.g. being in the same segment, or the binary
	/// being statically linked.
	#[inline(always)]
	pub unsafe fn from(ptr: *const ()) -> Self {
		let base = relative_code_base as usize;
		// println!("from: {}: {}", base, type_name::<T>());
		Self::new((ptr as usize).wrapping_sub(base))
	}
	/// Get back a `*const ()` from a `Code<T>`.
	#[inline(always)]
	pub fn to(&self) -> *const () {
		let base = relative_code_base as usize;
		// println!("to: {}: {}", base, type_name::<T>());
		base.wrapping_add(self.0) as *const ()
	}
}
impl<T: ?Sized> Clone for Code<T> {
	#[inline(always)]
	fn clone(&self) -> Self {
		Self(self.0, marker::PhantomData)
	}
}
impl<T: ?Sized> Copy for Code<T> {}
impl<T: ?Sized> PartialEq for Code<T> {
	#[inline(always)]
	fn eq(&self, other: &Self) -> bool {
		self.0 == other.0
	}
}
impl<T: ?Sized> Eq for Code<T> {}
impl<T: ?Sized> hash::Hash for Code<T> {
	#[inline(always)]
	fn hash<H: hash::Hasher>(&self, state: &mut H) {
		self.0.hash(state)
	}
}
impl<T: ?Sized> PartialOrd for Code<T> {
	#[inline(always)]
	fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
		self.0.partial_cmp(&other.0)
	}
}
impl<T: ?Sized> Ord for Code<T> {
	#[inline(always)]
	fn cmp(&self, other: &Self) -> cmp::Ordering {
		self.0.cmp(&other.0)
	}
}
impl<T: ?Sized> fmt::Debug for Code<T> {
	fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
		f.debug_struct("Code")
			.field(type_name::<T>(), &self.0)
			.finish()
	}
}
impl<T: ?Sized + 'static> Serialize for Code<T> {
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
impl<'de, T: ?Sized + 'static> Deserialize<'de> for Code<T> {
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

/// Wraps `&'static` references such that they can be safely sent between other
/// processes running the same binary.
///
/// For references into the data and BSS segments.
///
/// The base used is the address of a zero sized static item:
/// ```ignore
/// #[used]
/// #[no_mangle]
/// pub static RELATIVE_DATA_BASE: () = ();
///
/// let base = &RELATIVE_DATA_BASE as *const () as usize;
/// ```
pub struct Data<T>(usize, marker::PhantomData<fn(T)>);
impl<T> Data<T> {
	#[inline(always)]
	fn new(p: usize) -> Self {
		Self(p, marker::PhantomData)
	}
	/// Create a `Data<T>` from a `&'static T`.
	///
	/// This is unsafe as it is up to the user to ensure the pointer lies within
	/// static memory.
	///
	/// i.e. the pointer needs to be positioned the same relative to the base in
	/// every invocation, through e.g. being in the same segment, or the binary
	/// being statically linked.
	#[inline(always)]
	pub unsafe fn from(ptr: &'static T) -> Self {
		let base = {
			let base: *const () = &RELATIVE_DATA_BASE;
			base
		} as usize;
		// println!("from: {}: {}", base, type_name::<T>());
		Self::new(
			({
				let ptr: *const T = ptr;
				ptr
			} as usize)
				.wrapping_sub(base),
		)
	}
	/// Get back a `&'static T` from a `Data<T>`.
	#[inline(always)]
	pub fn to(&self) -> &'static T {
		let base = {
			let base: *const () = &RELATIVE_DATA_BASE;
			base
		} as usize;
		// println!("to: {}: {}", base, type_name::<T>());
		unsafe { &*(base.wrapping_add(self.0) as *const T) }
	}
}
impl<T> Clone for Data<T> {
	#[inline(always)]
	fn clone(&self) -> Self {
		Self(self.0, marker::PhantomData)
	}
}
impl<T> Copy for Data<T> {}
impl<T> PartialEq for Data<T> {
	#[inline(always)]
	fn eq(&self, other: &Self) -> bool {
		self.0 == other.0
	}
}
impl<T> Eq for Data<T> {}
impl<T> hash::Hash for Data<T> {
	#[inline(always)]
	fn hash<H: hash::Hasher>(&self, state: &mut H) {
		self.0.hash(state)
	}
}
impl<T> PartialOrd for Data<T> {
	#[inline(always)]
	fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
		self.0.partial_cmp(&other.0)
	}
}
impl<T> Ord for Data<T> {
	#[inline(always)]
	fn cmp(&self, other: &Self) -> cmp::Ordering {
		self.0.cmp(&other.0)
	}
}
impl<T> fmt::Debug for Data<T> {
	fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
		f.debug_struct("Data")
			.field(type_name::<T>(), &self.0)
			.finish()
	}
}
impl<T: 'static> Serialize for Data<T> {
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
impl<'de, T: 'static> Deserialize<'de> for Data<T> {
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
/// pub static RELATIVE_VTABLE_BASE: &(dyn any::Any + Sync) = &();
///
/// let base = mem::transmute::<*const dyn any::Any, std::raw::TraitObject>(RELATIVE_VTABLE_BASE).vtable as usize;
/// ```
pub struct Vtable<T: ?Sized>(usize, marker::PhantomData<fn(T)>);
impl<T: ?Sized> Vtable<T> {
	#[inline(always)]
	fn new(p: usize) -> Self {
		Self(p, marker::PhantomData)
	}
	/// Create a `Vtable<T>` from a `&'static ()`.
	///
	/// This is unsafe as it is up to the user to ensure the pointer lies within
	/// static memory.
	///
	/// i.e. the pointer needs to be positioned the same relative to the base in
	/// every invocation, through e.g. being in the same segment, or the binary
	/// being statically linked.
	#[inline(always)]
	pub unsafe fn from(ptr: &'static ()) -> Self {
		let base = mem::transmute::<*const dyn any::Any, TraitObject>(RELATIVE_VTABLE_BASE).vtable
			as usize;
		#[cfg(feature = "nightly")]
		{
			let check_base =
				mem::transmute::<*const dyn any::Any, std::raw::TraitObject>(RELATIVE_VTABLE_BASE)
					.vtable as usize;
			assert_eq!(check_base, base);
		}
		// let data_base = &RELATIVE_DATA_BASE as *const () as usize;
		// println!("from: {}: {}", base.wrapping_sub(data_base), unsafe{type_name::<T>()});
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
		let base =
			unsafe { mem::transmute::<*const dyn any::Any, TraitObject>(RELATIVE_VTABLE_BASE) }
				.vtable as usize;
		#[cfg(feature = "nightly")]
		{
			let check_base = unsafe {
				mem::transmute::<*const dyn any::Any, std::raw::TraitObject>(RELATIVE_VTABLE_BASE)
			}
			.vtable as usize;
			assert_eq!(check_base, base);
		}
		// let data_base = &RELATIVE_DATA_BASE as *const () as usize;
		// println!("to: {}: {}", base.wrapping_sub(data_base), unsafe{type_name::<T>()});
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
					&& self.b == other.b && self.c == other.c
					&& self.d == other.d && self.e == other.e
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
}
