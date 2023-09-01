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

use std::{marker::PhantomData, mem, ptr, sync::atomic::Ordering};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
#[repr(C, packed)]
pub(crate) struct PairedPointer<T> {
  pub data: *mut T,
  pub counter: usize,
}

impl<T> PairedPointer<T> {
  pub fn new() -> Self {
    Self {
      data: ptr::null_mut(),
      counter: 0,
    }
  }
}

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

pub fn is_lock_free() -> bool {
  AtomicPtr::<u8>::is_lock_free() && AtomicKey::is_lock_free()
}

struct Node<T> {
  pub value: mem::ManuallyDrop<T>,
  pub next: *mut Node<T>,
}

impl<T> Node<T> {
  pub fn for_value(value: T) -> Self {
    Self {
      value: mem::ManuallyDrop::new(value),
      next: ptr::null_mut(),
    }
  }
}

pub struct Stack<T> {
  top: AtomicKey,
  _phantom: PhantomData<T>,
}

impl<T> Stack<T> {
  pub fn new() -> Self {
    let p = PairedPointer::<Node<T>>::new();
    Self {
      top: AtomicKey::new(unsafe { p.into_raw() }),
      _phantom: PhantomData,
    }
  }

  pub fn push(&self, x: T) {
    let mut cur_top: Key = self.top.load(Ordering::Acquire);
    let target: *mut Node<T> = Box::into_raw(Box::new(Node::for_value(x)));

    loop {
      let new_top: Key = {
        let PairedPointer { data, counter } =
          unsafe { PairedPointer::<Node<T>>::from_raw(cur_top) };
        unsafe {
          (*target).next = data;
        }
        let new_top: PairedPointer<Node<T>> = PairedPointer {
          data: target,
          counter: counter.wrapping_add(1),
        };
        unsafe { new_top.into_raw() }
      };

      match self
        .top
        .compare_exchange_weak(cur_top, new_top, Ordering::Release, Ordering::Relaxed)
      {
        Ok(_) => {
          break;
        },
        Err(external_cur_top) => {
          cur_top = external_cur_top;
        },
      }
    }
  }

  pub fn pop(&self) -> Option<T> {
    let mut cur_top: Key = self.top.load(Ordering::Acquire);

    loop {
      let (new_top, mut top_ptr): (Key, ptr::NonNull<Node<T>>) = {
        let PairedPointer { data, counter } =
          unsafe { PairedPointer::<Node<T>>::from_raw(cur_top) };

        let mut top_ptr: ptr::NonNull<Node<T>> = ptr::NonNull::new(data)?;

        let new_top: PairedPointer<Node<T>> = PairedPointer {
          data: unsafe { top_ptr.as_mut().next },
          counter: counter.wrapping_add(1),
        };
        let new_top = unsafe { new_top.into_raw() };
        (new_top, top_ptr)
      };

      match self
        .top
        .compare_exchange_weak(cur_top, new_top, Ordering::AcqRel, Ordering::Acquire)
      {
        Ok(_) => {
          /* NB: Box will get dropped, without dropping the T since it's ManuallyDrop. */
          let mut top_box: Box<Node<T>> = unsafe { Box::from_raw(top_ptr.as_mut()) };
          let val: T = unsafe { mem::ManuallyDrop::take(&mut top_box.value) };
          break Some(val);
        },
        Err(external_cur_top) => {
          cur_top = external_cur_top;
        },
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use std::ptr;

  #[test]
  fn check_lock_free() {
    assert!(is_lock_free());
  }

  #[test]
  fn try_round_trip() {
    let x: PairedPointer<u8> = PairedPointer {
      data: ptr::null_mut(),
      counter: 3,
    };
    let y: PairedPointer<u8> = unsafe { PairedPointer::from_raw(x.clone().into_raw()) };
    assert_eq!(x, y);
  }

  #[test]
  fn push_pop() {
    let x: Stack<u8> = Stack::new();
    assert_eq!(x.pop(), None);
    x.push(1);
    assert_eq!(x.pop(), Some(1));
    assert_eq!(x.pop(), None);
  }
}
