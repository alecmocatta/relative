//! A type to wrap `&'static` references such that they can be safely sent
//! between other processes running the same binary.
//!
//! **[Crates.io](https://crates.io/crates/relative) │ [Repo](https://github.com/alecmocatta/relative)**
//!
//! References are adjusted relative to a base when (de)serialised, which
//! accounts for binaries being dynamically loaded at different addresses under
//! multiple invocations.
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
//! let relative = unsafe{Pointer::from(x)};
//! // send `relative` to remote...
//! ```
//! ### Remote process
//! ```
//! # use relative::*;
//! # let x: &'static [u16;4] = &[2,3,5,8];
//! # let relative = unsafe{Pointer::from(x)};
//! // receive `relative`
//! println!("{:?}", relative.to());
//! // prints "[2, 3, 5, 8]"
//! ```
//!
//! # Note
//!
//! This currently requires Rust nightly.

#![doc(html_root_url = "https://docs.rs/relative/0.1.0")]
#![feature(used, core_intrinsics, raw, specialization)]
#![deny(missing_docs, warnings, deprecated)]
#![allow(intra_doc_link_resolution_failure)]

extern crate build_id;
extern crate metatype;
extern crate serde;
extern crate uuid;
#[cfg(test)]
#[macro_use]
extern crate serde_derive;
#[cfg(test)]
extern crate bincode;
#[cfg(test)]
extern crate serde_json;

#[cfg(test)]
mod tests;

use std::{cmp, fmt, hash, intrinsics, marker, mem, raw};

/// Implemented on all `T: Sized + 'static`, this provides the base memory
/// address that `&'static T` references are (de)serialised relative to.
pub trait Static: 'static {
	/// Provide the base memory address that `&'static Self` references are
	/// (de)serialised relative to.
	fn base() -> usize;
}

/// For references into the data and BSS segments
///
/// The base used is a zero sized static item marked as `#[used]`.
pub struct Data;
#[used]
static DATA_BASE: () = ();
impl Static for Data {
	#[inline(always)]
	fn base() -> usize {
		&DATA_BASE as *const () as usize
	}
}
impl<T: 'static> Static for T {
	#[inline(always)]
	default fn base() -> usize {
		Data::base()
	}
}
/// For references into the code segment
///
/// The base used is a fn item monomorphised for `T` marked as `#[used]
/// #[inline(never)]`.
pub struct Code<T: ?Sized + 'static>(marker::PhantomData<fn(T)>);
impl<T: ?Sized + 'static> Code<T> {
	#[used]
	#[inline(never)]
	fn abc(_: &T) {
		unsafe { intrinsics::unreachable() };
	}
}
impl<T: ?Sized + 'static> Static for Code<T> {
	#[inline(always)]
	fn base() -> usize {
		<Code<T>>::abc as fn(&T) as usize
	}
}
#[doc(hidden)]
pub struct Vtable<T: ?Sized + 'static>(marker::PhantomData<fn(T)>);
impl<T: ?Sized + 'static> Vtable<T> {
	#[used]
	#[inline(never)]
	fn abc(_: &T) {
		unsafe { intrinsics::unreachable() };
	}
}
impl<T: ?Sized + 'static> Static for Vtable<T> {
	#[inline(always)]
	fn base() -> usize {
		unsafe {
			mem::transmute::<*const Fn(&T), raw::TraitObject>(
				&(<Vtable<T>>::abc as fn(&T)) as &Fn(&T),
			)
		}.vtable as usize
	}
}

/// Wraps `&'static` references such that they can be safely sent between other
/// processes running the same binary.
pub struct Pointer<T: Static>(usize, marker::PhantomData<fn(T)>);
impl<T: Static> Pointer<T> {
	#[inline(always)]
	fn new(p: usize) -> Pointer<T> {
		Pointer(p, marker::PhantomData)
	}
	/// Create a `Pointer<T>` from a `&'static T`.
	///
	/// This is unsafe as it is up to the user to ensure the pointer lies within
	/// static memory.
	#[inline(always)]
	pub unsafe fn from(ptr: &'static T) -> Pointer<T> {
		let base = T::base();
		// println!("from: {}: {}", base, intrinsics::type_name::<T>());
		Pointer::new((ptr as *const T as usize).wrapping_sub(base))
	}
	/// Get back a `&'static T` from a `Pointer<T>`.
	#[inline(always)]
	pub fn to(&self) -> &'static T {
		let base = T::base();
		// println!("to: {}: {}", base, intrinsics::type_name::<T>());
		unsafe { &*(base.wrapping_add(self.0) as *const T) }
	}
}
impl<T: Static> Clone for Pointer<T> {
	#[inline(always)]
	fn clone(&self) -> Self {
		Pointer(self.0, marker::PhantomData)
	}
}
impl<T: Static> Copy for Pointer<T> {}
impl<T: Static> PartialEq for Pointer<T> {
	#[inline(always)]
	fn eq(&self, other: &Self) -> bool {
		self.0 == other.0
	}
}
impl<T: Static> Eq for Pointer<T> {}
impl<T: Static> hash::Hash for Pointer<T> {
	#[inline(always)]
	fn hash<H: hash::Hasher>(&self, state: &mut H) {
		self.0.hash(state)
	}
}
impl<T: Static> cmp::PartialOrd for Pointer<T> {
	#[inline(always)]
	fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
		self.0.partial_cmp(&other.0)
	}
}
impl<T: Static> cmp::Ord for Pointer<T> {
	#[inline(always)]
	fn cmp(&self, other: &Self) -> cmp::Ordering {
		self.0.cmp(&other.0)
	}
}
impl<T: Static> fmt::Debug for Pointer<T> {
	fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
		f.debug_struct("Pointer")
			.field(unsafe { intrinsics::type_name::<T>() }, &self.0)
			.finish()
	}
}
impl<T: Static> serde::ser::Serialize for Pointer<T> {
	#[inline(always)]
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer,
	{
		<(uuid::Uuid, u64, usize) as serde::ser::Serialize>::serialize(
			&(
				build_id::get(),
				unsafe { intrinsics::type_id::<T>() },
				self.0,
			),
			serializer,
		)
	}
}
impl<'de, T: Static> serde::de::Deserialize<'de> for Pointer<T> {
	#[inline(always)]
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		<(uuid::Uuid, u64, usize) as serde::de::Deserialize<'de>>::deserialize(deserializer)
			.and_then(|(build, id, ptr)| {
				let local = build_id::get();
				if build == local {
					if id == unsafe { intrinsics::type_id::<T>() } {
						Ok(Pointer::new(ptr))
					} else {
						Err(serde::de::Error::custom(format_args!(
							"relative reference to wrong type ???:{}, expected {}:{}",
							id,
							unsafe { intrinsics::type_name::<T>() },
							unsafe { intrinsics::type_id::<T>() }
						)))
					}
				} else {
					Err(serde::de::Error::custom(format_args!(
						"relative reference came from a different binary {}, expected {}",
						build, local
					)))
				}
			})
	}
}
