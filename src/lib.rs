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

#![doc(html_root_url = "https://docs.rs/relative/0.1.3")]
#![feature(core_intrinsics, raw, used)]
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
	stable_features,
	clippy::inline_always,
	clippy::doc_markdown,
	clippy::trivially_copy_pass_by_ref
)]

#[cfg(test)]
mod tests;

use std::{any, cmp, fmt, hash, intrinsics, marker, mem, raw};

#[doc(hidden)]
#[used]
#[no_mangle]
pub static RELATIVE_DATA_BASE: () = ();

#[doc(hidden)]
#[no_mangle]
#[inline(never)]
pub fn relative_code_base() {
	unsafe { intrinsics::unreachable() }
}

#[doc(hidden)]
#[used]
#[no_mangle]
pub static RELATIVE_VTABLE_BASE: &(dyn any::Any + Sync) = &();

/// Wraps function pointers such that they can be safely sent between other
/// processes running the same binary.
///
/// For references into the code aka text segment.
///
/// The base used is the address of a function:
/// ```rust,ignore
/// #[no_mangle]
/// #[inline(never)]
/// pub fn relative_code_base() {
/// 	unsafe { intrinsics::unreachable() }
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
		// println!("from: {}: {}", base, intrinsics::type_name::<T>());
		Self::new((ptr as usize).wrapping_sub(base))
	}
	/// Get back a `*const ()` from a `Code<T>`.
	#[inline(always)]
	pub fn to(&self) -> *const () {
		let base = relative_code_base as usize;
		// println!("to: {}: {}", base, intrinsics::type_name::<T>());
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
			.field(unsafe { intrinsics::type_name::<T>() }, &self.0)
			.finish()
	}
}
impl<T: ?Sized + 'static> serde::ser::Serialize for Code<T> {
	#[inline]
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
impl<'de, T: ?Sized + 'static> serde::de::Deserialize<'de> for Code<T> {
	#[inline]
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		<(uuid::Uuid, u64, usize) as serde::de::Deserialize<'de>>::deserialize(deserializer)
			.and_then(|(build, id, ptr)| {
				let local = build_id::get();
				if build == local {
					if id == unsafe { intrinsics::type_id::<T>() } {
						Ok(Self::new(ptr))
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

/// Wraps `&'static` references such that they can be safely sent between other
/// processes running the same binary.
///
/// For references into the data and BSS segments.
///
/// The base used is the address of a zero sized static item:
/// ```rust,ignore
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
		// println!("from: {}: {}", base, intrinsics::type_name::<T>());
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
		// println!("to: {}: {}", base, intrinsics::type_name::<T>());
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
			.field(unsafe { intrinsics::type_name::<T>() }, &self.0)
			.finish()
	}
}
impl<T: 'static> serde::ser::Serialize for Data<T> {
	#[inline]
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
impl<'de, T: 'static> serde::de::Deserialize<'de> for Data<T> {
	#[inline]
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		<(uuid::Uuid, u64, usize) as serde::de::Deserialize<'de>>::deserialize(deserializer)
			.and_then(|(build, id, ptr)| {
				let local = build_id::get();
				if build == local {
					if id == unsafe { intrinsics::type_id::<T>() } {
						Ok(Self::new(ptr))
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

/// Wraps `&'static` references to vtables such that they can be safely sent
/// between other processes running the same binary.
///
/// For references into the segment that houses the vtables, typically the
/// read-only data segment aka rodata.
///
/// The base used is the vtable of a static trait object:
/// ```rust,ignore
/// #[doc(hidden)]
/// #[used]
/// #[no_mangle]
/// pub static RELATIVE_VTABLE_BASE: &(dyn any::Any+Sync) = &() as &(dyn any::Any+Sync);
///
/// let base = mem::transmute::<*const dyn any::Any, raw::TraitObject>(RELATIVE_VTABLE_BASE).vtable as usize;
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
		let base = mem::transmute::<*const dyn any::Any, raw::TraitObject>(RELATIVE_VTABLE_BASE)
			.vtable as usize;
		// let data_base = &RELATIVE_DATA_BASE as *const () as usize;
		// println!("from: {}: {}", base.wrapping_sub(data_base), unsafe{intrinsics::type_name::<T>()});
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
		let base = unsafe {
			mem::transmute::<*const dyn any::Any, raw::TraitObject>(RELATIVE_VTABLE_BASE)
		}
		.vtable as usize;
		// let data_base = &RELATIVE_DATA_BASE as *const () as usize;
		// println!("to: {}: {}", base.wrapping_sub(data_base), unsafe{intrinsics::type_name::<T>()});
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
			.field(unsafe { intrinsics::type_name::<T>() }, &self.0)
			.finish()
	}
}
impl<T: ?Sized + 'static> serde::ser::Serialize for Vtable<T> {
	#[inline]
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
impl<'de, T: ?Sized + 'static> serde::de::Deserialize<'de> for Vtable<T> {
	#[inline]
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		<(uuid::Uuid, u64, usize) as serde::de::Deserialize<'de>>::deserialize(deserializer)
			.and_then(|(build, id, ptr)| {
				let local = build_id::get();
				if build == local {
					if id == unsafe { intrinsics::type_id::<T>() } {
						Ok(Self::new(ptr))
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
