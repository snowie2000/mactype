//! COM apartment lifetime.

use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use windows::Win32::System::Com::{
    CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_MULTITHREADED,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComThreading {
    Apartment,
    Multi,
}

/// A COM initialization on the current thread, uninitialized on drop. The
/// value is `!Send` by construction (it holds a raw pointer marker), so it
/// cannot leave the thread it initialized.
#[derive(Debug)]
pub struct ComApartment {
    owned: bool,
    _thread_bound: std::marker::PhantomData<*const ()>,
}

impl ComApartment {
    /// Initializes COM with `threading`. The error is the raw `HRESULT`.
    pub fn initialize(threading: ComThreading) -> Result<Self, i32> {
        let model = match threading {
            ComThreading::Apartment => COINIT_APARTMENTTHREADED,
            ComThreading::Multi => COINIT_MULTITHREADED,
        };
        // SAFETY: no reserved pointer is passed; the call only affects the
        // current thread's COM state, which this value now owns.
        unsafe { CoInitializeEx(None, model) }
            .ok()
            .map_err(|error| error.code().0)?;
        Ok(Self {
            owned: true,
            _thread_bound: std::marker::PhantomData,
        })
    }

    /// Initializes COM or borrows an apartment already initialized with another
    /// threading model. A borrowed value does not balance an initialization.
    pub fn initialize_or_borrow(threading: ComThreading) -> Result<Self, i32> {
        let model = match threading {
            ComThreading::Apartment => COINIT_APARTMENTTHREADED,
            ComThreading::Multi => COINIT_MULTITHREADED,
        };
        // SAFETY: no reserved pointer is passed; the call affects only this
        // thread's COM state. Successful initialization is balanced on drop.
        let result = unsafe { CoInitializeEx(None, model) };
        if result.is_ok() {
            Ok(Self {
                owned: true,
                _thread_bound: std::marker::PhantomData,
            })
        } else if result == RPC_E_CHANGED_MODE {
            Ok(Self {
                owned: false,
                _thread_bound: std::marker::PhantomData,
            })
        } else {
            Err(result.0)
        }
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.owned {
            // SAFETY: an owned value is the matching initialization for the
            // current thread and is dropped there, so the balance stays exact.
            unsafe { CoUninitialize() };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ComApartment, ComThreading};
    use crate::read_shortcut;

    #[test]
    fn an_apartment_initializes_and_uninitializes_on_a_fresh_thread() {
        std::thread::spawn(|| {
            let apartment = ComApartment::initialize(ComThreading::Apartment).unwrap();
            drop(apartment);
            let again = ComApartment::initialize(ComThreading::Multi).unwrap();
            drop(again);
        })
        .join()
        .unwrap();
    }

    #[test]
    fn a_changed_mode_borrows_the_existing_apartment() {
        std::thread::spawn(|| {
            let apartment = ComApartment::initialize(ComThreading::Apartment).unwrap();
            let borrowed = ComApartment::initialize_or_borrow(ComThreading::Multi).unwrap();
            drop(borrowed);
            let directory = tempfile::tempdir().unwrap();
            assert!(read_shortcut(&directory.path().join("missing.lnk")).is_none());
            drop(apartment);
        })
        .join()
        .unwrap();
    }
}
