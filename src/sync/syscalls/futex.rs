use core::ffi::c_uint;
use core::num::NonZero;
use core::ptr;
use core::sync::atomic::AtomicU32;

/// `FUTEX_WAITERS`
pub const FUTEX_WAITERS: u32 = linux_raw_sys::general::FUTEX_WAITERS;
/// `FUTEX_OWNER_DIED`
pub const FUTEX_OWNER_DIED: u32 = linux_raw_sys::general::FUTEX_OWNER_DIED;

pub type Timespec = linux_raw_sys::general::__kernel_timespec;

bitflags::bitflags! {
		#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
		pub struct FutexFlags: u32 {
				const PRIVATE = linux_raw_sys::general::FUTEX_PRIVATE_FLAG;
				const CLOCK_REALTIME = linux_raw_sys::general::FUTEX_CLOCK_REALTIME;
		}
}

#[inline]
unsafe fn futex_val2<const OP: c_uint>(
	uaddr: *const AtomicU32,
	flags: FutexFlags,
	val: u32,
	val2: u32,
	uaddr2: *const AtomicU32,
	val3: u32,
) -> Result<usize, syscalls::Errno> {
	debug_assert!((FutexFlags::all() & !FutexFlags::CLOCK_REALTIME).contains(flags));

	syscalls::syscall!(
		syscalls::Sysno::futex,
		uaddr,
		OP | flags.bits(),
		val,
		val2 as usize as *const Timespec,
		uaddr2,
		val3
	)
}

#[inline]
unsafe fn futex_timeout<const OP: c_uint>(
	uaddr: *const AtomicU32,
	flags: FutexFlags,
	val: u32,
	timeout: Option<Timespec>,
	uaddr2: *const AtomicU32,
	val3: u32,
) -> Result<usize, syscalls::Errno> {
	debug_assert!(FutexFlags::all().contains(flags));

	syscalls::syscall!(
		syscalls::Sysno::futex,
		uaddr,
		OP | flags.bits(),
		val,
		timeout
			.as_ref()
			.map(|timeout| timeout as *const Timespec)
			.unwrap_or(ptr::null()),
		uaddr2,
		val3
	)
}

/// Equivalent to `syscall(SYS_futex, uaddr, FUTEX_WAIT, val, timeout, NULL, 0)`
#[inline]
pub unsafe fn futex_wait(
	uaddr: &AtomicU32,
	flags: FutexFlags,
	val: u32,
	timeout: Option<Timespec>,
) -> Result<(), syscalls::Errno> {
	futex_timeout::<{ linux_raw_sys::general::FUTEX_WAIT }>(uaddr, flags, val, timeout, ptr::null(), 0).map(|ret| {
		debug_assert_eq!(ret, 0);
	})
}

/// Equivalent to `syscall(SYS_futex, uaddr, FUTEX_WAKE, val, NULL, NULL, 0)`
#[inline]
pub unsafe fn futex_wake(uaddr: &AtomicU32, flags: FutexFlags, val: u32) -> Result<usize, syscalls::Errno> {
	futex_val2::<{ linux_raw_sys::general::FUTEX_WAKE }>(uaddr, flags, val, 0, ptr::null(), 0)
}

/// Equivalent to `syscall(SYS_futex, uaddr, FUTEX_REQUEUE, val, val2, uaddr2, 0)`
#[inline]
pub unsafe fn futex_requeue(
	uaddr: &AtomicU32,
	flags: FutexFlags,
	val: u32,
	val2: u32,
	uaddr2: &AtomicU32,
) -> Result<usize, syscalls::Errno> {
	futex_val2::<{ linux_raw_sys::general::FUTEX_REQUEUE }>(uaddr, flags, val, val2, uaddr2, 0)
}

/// Equivalent to `syscall(SYS_futex, uaddr, FUTEX_CMP_REQUEUE, val, val2, uaddr2, val3)`
#[inline]
pub unsafe fn futex_cmp_requeue(
	uaddr: &AtomicU32,
	flags: FutexFlags,
	val: u32,
	val2: u32,
	uaddr2: &AtomicU32,
	val3: u32,
) -> Result<usize, syscalls::Errno> {
	futex_val2::<{ linux_raw_sys::general::FUTEX_CMP_REQUEUE }>(uaddr, flags, val, val2, uaddr2, val3)
}

/// `FUTEX_OP_*` operations for use with [`wake_op`].
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(u32)]
pub enum FutexWakeOp {
	/// `FUTEX_OP_SET`: `uaddr2 = oparg;`
	Set = 0,
	/// `FUTEX_OP_ADD`: `uaddr2 += oparg;`
	Add = 1,
	/// `FUTEX_OP_OR`: `uaddr2 |= oparg;`
	Or = 2,
	/// `FUTEX_OP_ANDN`: `uaddr2 &= ~oparg;`
	AndN = 3,
	/// `FUTEX_OP_XOR`: `uaddr2 ^= oparg;`
	XOr = 4,
	/// `FUTEX_OP_SET | FUTEX_OP_ARG_SHIFT`: `uaddr2 = (oparg << 1);`
	#[allow(clippy::identity_op)]
	SetShift = 0 | 8,
	/// `FUTEX_OP_ADD | FUTEX_OP_ARG_SHIFT`: `uaddr2 += (oparg << 1);`
	AddShift = 1 | 8,
	/// `FUTEX_OP_OR | FUTEX_OP_ARG_SHIFT`: `uaddr2 |= (oparg << 1);`
	OrShift = 2 | 8,
	/// `FUTEX_OP_ANDN | FUTEX_OP_ARG_SHIFT`: `uaddr2 &= !(oparg << 1);`
	AndNShift = 3 | 8,
	/// `FUTEX_OP_XOR | FUTEX_OP_ARG_SHIFT`: `uaddr2 ^= (oparg << 1);`
	XOrShift = 4 | 8,
}

/// `FUTEX_OP_CMP_*` operations for use with [`wake_op`].
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(u32)]
pub enum FutexWakeOpCmp {
	/// `FUTEX_OP_CMP_EQ`: `if oldval == cmparg { wake(); }`
	Eq = 0,
	/// `FUTEX_OP_CMP_EQ`: `if oldval != cmparg { wake(); }`
	Ne = 1,
	/// `FUTEX_OP_CMP_EQ`: `if oldval < cmparg { wake(); }`
	Lt = 2,
	/// `FUTEX_OP_CMP_EQ`: `if oldval <= cmparg { wake(); }`
	Le = 3,
	/// `FUTEX_OP_CMP_EQ`: `if oldval > cmparg { wake(); }`
	Gt = 4,
	/// `FUTEX_OP_CMP_EQ`: `if oldval >= cmparg { wake(); }`
	Ge = 5,
}

/// Equivalent to `syscall(SYS_futex, uaddr, FUTEX_WAKE_OP, val, val2, uaddr2, val3)`
#[inline]
pub unsafe fn futex_wake_op(
	uaddr: &AtomicU32,
	flags: FutexFlags,
	val: u32,
	val2: u32,
	uaddr2: &AtomicU32,
	op: FutexWakeOp,
	cmp: FutexWakeOpCmp,
	oparg: u16,
	cmparg: u16,
) -> Result<usize, syscalls::Errno> {
	if oparg >= 1 << 12 || cmparg >= 1 << 12 {
		return Err(syscalls::Errno::EINVAL);
	}

	let val3 = ((op as u32) << 28) | ((cmp as u32) << 24) | ((oparg as u32) << 12) | (cmparg as u32);

	futex_val2::<{ linux_raw_sys::general::FUTEX_WAKE_OP }>(uaddr, flags, val, val2, uaddr2, val3)
}

/// Equivalent to `syscall(SYS_futex, uaddr, FUTEX_LOCK_PI, 0, timeout, NULL, 0)`
#[inline]
pub unsafe fn futex_lock_pi(
	uaddr: &AtomicU32,
	flags: FutexFlags,
	timeout: Option<Timespec>,
) -> Result<(), syscalls::Errno> {
	futex_timeout::<{ linux_raw_sys::general::FUTEX_LOCK_PI }>(uaddr, flags, 0, timeout, ptr::null(), 0).map(|ret| {
		debug_assert_eq!(ret, 0);
	})
}

/// Equivalent to `syscall(SYS_futex, uaddr, FUTEX_UNLOCK_PI, 0, NULL, NULL, 0)`
#[inline]
pub unsafe fn futex_unlock_pi(uaddr: &AtomicU32, flags: FutexFlags) -> Result<(), syscalls::Errno> {
	futex_val2::<{ linux_raw_sys::general::FUTEX_UNLOCK_PI }>(uaddr, flags, 0, 0, ptr::null(), 0).map(|ret| {
		debug_assert_eq!(ret, 0);
	})
}

/// Equivalent to `syscall(SYS_futex, uaddr, FUTEX_TRYLOCK_PI, 0, NULL, NULL, 0)`
#[inline]
pub unsafe fn futex_trylock_pi(uaddr: &AtomicU32, flags: FutexFlags) -> Result<bool, syscalls::Errno> {
	futex_val2::<{ linux_raw_sys::general::FUTEX_TRYLOCK_PI }>(uaddr, flags, 0, 0, ptr::null(), 0).map(|ret| ret == 0)
}

/// Equivalent to `syscall(SYS_futex, uaddr, FUTEX_WAIT_BITSET, val, timeout/val2, NULL, val3)`
#[inline]
pub unsafe fn futex_wait_bitset(
	uaddr: &AtomicU32,
	flags: FutexFlags,
	val: u32,
	timeout: Option<Timespec>,
	val3: NonZero<u32>,
) -> Result<(), syscalls::Errno> {
	futex_timeout::<{ linux_raw_sys::general::FUTEX_WAIT_BITSET }>(uaddr, flags, val, timeout, ptr::null(), val3.get())
		.map(|ret| {
			debug_assert_eq!(ret, 0);
		})
}

/// Equivalent to `syscall(SYS_futex, uaddr, FUTEX_WAKE_BITSET, val, NULL, NULL, val3)`
#[inline]
pub unsafe fn futex_wake_bitset(
	uaddr: &AtomicU32,
	flags: FutexFlags,
	val: u32,
	val3: NonZero<u32>,
) -> Result<usize, syscalls::Errno> {
	futex_val2::<{ linux_raw_sys::general::FUTEX_WAKE_BITSET }>(uaddr, flags, val, 0, ptr::null(), val3.get())
}

/// Equivalent to `syscall(SYS_futex, uaddr, FUTEX_WAIT_REQUEUE_PI, val, timeout, uaddr2, 0)`
#[inline]
pub unsafe fn futex_wait_requeue_pi(
	uaddr: &AtomicU32,
	flags: FutexFlags,
	val: u32,
	timeout: Option<Timespec>,
	uaddr2: &AtomicU32,
) -> Result<(), syscalls::Errno> {
	futex_timeout::<{ linux_raw_sys::general::FUTEX_WAIT_REQUEUE_PI }>(uaddr, flags, val, timeout, uaddr2, 0).map(|ret| {
		debug_assert_eq!(ret, 0);
	})
}

/// Equivalent to `syscall(SYS_futex, uaddr, FUTEX_CMP_REQUEUE_PI, 1, val2, uaddr2, val3)`
#[inline]
pub unsafe fn futex_cmp_requeue_pi(
	uaddr: &AtomicU32,
	flags: FutexFlags,
	val2: u32,
	uaddr2: &AtomicU32,
	val3: u32,
) -> Result<usize, syscalls::Errno> {
	futex_val2::<{ linux_raw_sys::general::FUTEX_CMP_REQUEUE_PI }>(uaddr, flags, 1, val2, uaddr2, val3)
}

/// Equivalent to `syscall(SYS_futex, uaddr, FUTEX_LOCK_PI2, 0, timeout, NULL, 0)`
#[inline]
pub unsafe fn futex_lock_pi2(
	uaddr: &AtomicU32,
	flags: FutexFlags,
	timeout: Option<Timespec>,
) -> Result<(), syscalls::Errno> {
	futex_timeout::<{ linux_raw_sys::general::FUTEX_LOCK_PI2 }>(uaddr, flags, 0, timeout, ptr::null(), 0).map(|ret| {
		debug_assert_eq!(ret, 0);
	})
}
