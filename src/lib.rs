/*
 * Description: ???
 *
 * Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
 * SPDX-License-Identifier: MIT OR Apache-2.0
 *
 * FIXME: is this sufficient license notice?
 */

//! ???

/* These clippy lint descriptions are purely non-functional and do not affect the functionality
 * or correctness of the code. */
// #![warn(missing_docs)]

/* Ensure any doctest warnings fails the doctest! */
#![doc(test(attr(deny(warnings))))]
/* Enable all clippy lints except for many of the pedantic ones. It's a shame this needs to be
 * copied and pasted across crates, but there doesn't appear to be a way to include inner
 * attributes from a common source. */
#![deny(
  clippy::all,
  clippy::default_trait_access,
  clippy::expl_impl_clone_on_copy,
  clippy::if_not_else,
  clippy::needless_continue,
  clippy::single_match_else,
  clippy::unseparated_literal_suffix,
  clippy::used_underscore_binding
)]
/* It is often more clear to show that nothing is being moved. */
#![allow(clippy::match_ref_pats)]
/* Subjective style. */
#![allow(
  clippy::derived_hash_with_manual_eq,
  clippy::len_without_is_empty,
  clippy::redundant_field_names,
  clippy::too_many_arguments,
  clippy::single_component_path_imports,
  clippy::double_must_use
)]
/* Default isn't as big a deal as people seem to think it is. */
#![allow(clippy::new_without_default, clippy::new_ret_no_self)]

use cfg_if::cfg_if;
use portable_atomic::AtomicPtr;
use static_assertions::assert_eq_size;

use std::mem;
#[cfg(test)]
use std::{cmp, sync::atomic::Ordering};

#[derive(Debug)]
#[repr(C)]
pub struct PairedPointer<T> {
  pub data: AtomicPtr<T>,
  pub counter: usize,
}

#[cfg(test)]
impl<T: Clone> Clone for PairedPointer<T> {
  fn clone(&self) -> Self {
    Self {
      data: AtomicPtr::new(self.data.load(Ordering::Acquire)),
      counter: self.counter,
    }
  }
}

#[cfg(test)]
impl<T: cmp::PartialEq> cmp::PartialEq for PairedPointer<T> {
  fn eq(&self, other: &Self) -> bool {
    self.data.load(Ordering::Acquire) == other.data.load(Ordering::Acquire)
      && self.counter == other.counter
  }
}

#[cfg(test)]
impl<T: cmp::Eq> cmp::Eq for PairedPointer<T> {}

cfg_if! {
  if #[cfg(target_pointer_width = "64")] {
    use portable_atomic::AtomicU128;

    pub type AtomicKey = AtomicU128;
    pub type Key = u128;
  } else if #[cfg(target_pointer_width = "32")] {
    use portable_atomic::AtomicU64;

    pub type AtomicKey = AtomicU64;
    pub type Key = u64;
  } else {
    compile_error!("unsupported pointer width");
  }
}

assert_eq_size!(PairedPointer<u8>, Key);

impl<T> PairedPointer<T> {
  pub unsafe fn from_raw(x: Key) -> Self {
    mem::transmute(x)
  }

  pub unsafe fn into_raw(self) -> Key {
    mem::transmute(self)
  }
}

/* pub struct Stack<T> {} */

#[cfg(test)]
mod tests {
  use super::*;

  use std::ptr;

  #[test]
  fn check_lock_free() {
    assert!(AtomicPtr::<u8>::is_lock_free());
    assert!(AtomicU128::is_lock_free());
  }

  #[test]
  fn try_round_trip() {
    let x: PairedPointer<u8> = PairedPointer {
      data: AtomicPtr::<u8>::new(ptr::null_mut()),
      counter: 3,
    };
    let y: PairedPointer<u8> = unsafe { PairedPointer::from_raw(x.clone().into_raw()) };
    assert_eq!(x, y);
  }
}
