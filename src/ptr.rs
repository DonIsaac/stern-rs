use core::mem::transmute;

use core::ptr::NonNull;

#[repr(transparent)]
pub struct ReadonlyNonNull<T: ?Sized>(NonNull<T>);

impl<T: ?Sized> ReadonlyNonNull<T> {
    #[inline]
    #[must_use]
    pub fn new(ptr: *const T) -> Option<Self> {
        Some(Self(NonNull::new(ptr as *mut T)?))
    }

    #[inline(always)]
    #[must_use]
    pub unsafe fn new_unchecked(ptr: *const T) -> Self {
        Self(NonNull::new_unchecked(ptr as *mut T))
    }

    #[inline(always)]
    #[must_use]
    pub const fn as_ptr(self) -> *const T {
        self.0.as_ptr()
    }

    #[inline(always)]
    #[must_use]
    pub const unsafe fn as_ref<'a>(&self) -> &'a T {
        self.0.as_ref()
    }

    #[inline(always)]
    #[must_use]
    pub const unsafe fn as_non_null(self) -> NonNull<T> {
        self.0
    }
}

impl<T: ?Sized> From<NonNull<T>> for ReadonlyNonNull<T> {
    fn from(value: NonNull<T>) -> Self {
        unsafe { transmute(value) }
    }
}
