use core::sync::atomic::{AtomicBool, Ordering};
use core::cell::UnsafeCell;

// DROP?
pub struct SpinLock<T> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}

/// SpinLock has to be Sync because other threads will need a reference to it to
/// acquire the lock.
///
/// Having a reference to SpinLock allows one to call `lock()` on it, getting a
/// mutable reference to inner type `T`. Thus `T` must be `Send`. `Sync` is not
/// necessary because multiple threads are not having concurrent access to the
/// inner `T`, at a time only one thread could have an exclusive reference to
/// the inner `T`.
unsafe impl<T: Send> Sync for SpinLock<T> {}

/// SpinLock is `Send` if `T` is `Send`. If another thread gets ownership of
/// spinlock then it could get an exclusive reference to inner `T`, thus
/// requiring the bound of `Send` for `T`.
unsafe impl<T: Send> Send for SpinLock<T> {}

impl<T> SpinLock<T> {
    pub const fn new(data: T) -> Self {
        SpinLock {
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(data),
        }
    }

    pub fn lock(&self) -> SpinGuard<'_, T> {
        loop {
            while self.locked.load(Ordering::Relaxed) {
                core::hint::spin_loop();
            }

            let exchange = self.locked.compare_exchange(false, 
                true, Ordering::Acquire, Ordering::Relaxed);

            match exchange {
                Ok(_) => {
                    break SpinGuard {
                        lock:     self,
                        __marker: core::marker::PhantomData,
                    };
                }
                Err(_) => {
                    continue;
                }
            }
        }
    }
}

pub struct SpinGuard<'lock, T> {
    lock:     &'lock SpinLock<T>,
    __marker: core::marker::PhantomData<*const ()>, // no sync send, covariant
}

impl<'lock, T> Drop for SpinGuard<'lock, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
    }
}

impl<'lock, T> core::ops::Deref for SpinGuard<'lock, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.data.get() }
    }
}

impl<'lock, T> core::ops::DerefMut for SpinGuard<'lock, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.data.get() }
    }
}
