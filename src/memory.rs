use libc::{
    self, HOST_VM_INFO64, HOST_VM_INFO64_COUNT, KERN_SUCCESS, c_int, c_void, host_statistics64,
    integer_t, mach_msg_type_number_t, mach_port_t, vm_statistics64,
};
use std::{mem, ptr};

#[derive(Debug, Clone, Default)]
pub struct MemoryStats {
    pub total_gb: f64,
    pub used_gb: f64,
    /// Percentage of memory currently in use (0-100)
    pub used_percent: u64,
    pub swap_total_gb: f64,
    pub swap_used_gb: f64,
}

pub struct MemoryReader {
    host_port: mach_port_t,
    page_size: u64,
    total_bytes: u64,
}

impl MemoryReader {
    pub fn new() -> Self {
        #[allow(deprecated)]
        let host_port = unsafe { libc::mach_host_self() };
        let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        let total_bytes = read_total_memory().unwrap_or(0);
        Self {
            host_port,
            page_size: if page_size > 0 {
                page_size as u64
            } else {
                4096
            },
            total_bytes,
        }
    }

    pub fn read(&mut self) -> MemoryStats {
        let mut stats: vm_statistics64 = unsafe { mem::zeroed() };
        let mut count: mach_msg_type_number_t = HOST_VM_INFO64_COUNT;
        let result = unsafe {
            host_statistics64(
                self.host_port,
                HOST_VM_INFO64,
                &mut stats as *mut vm_statistics64 as *mut integer_t,
                &mut count,
            )
        };
        if result != KERN_SUCCESS {
            return MemoryStats {
                total_gb: bytes_to_gb(self.total_bytes),
                ..MemoryStats::default()
            };
        }

        if self.total_bytes == 0 {
            self.total_bytes = read_total_memory().unwrap_or(0);
        }
        let page_size = self.page_size.max(4096);
        let active = stats.active_count as u64 * page_size;
        let wired = stats.wire_count as u64 * page_size;
        let compressed = stats.compressor_page_count as u64 * page_size;
        let inactive = stats.inactive_count as u64 * page_size;
        let mut free = stats.free_count as u64 * page_size;
        let speculative = stats.speculative_count as u64 * page_size;
        free = free.saturating_sub(speculative);
        let available = inactive.saturating_add(free);
        let total = if self.total_bytes > 0 {
            self.total_bytes
        } else {
            available
                .saturating_add(active)
                .saturating_add(wired)
                .saturating_add(compressed)
        };
        let used = total.saturating_sub(available);
        let used_percent = if total > 0 {
            ((total.saturating_sub(available)) as f64 / total as f64 * 100.0)
                .clamp(0.0, 100.0)
                .floor()
        } else {
            0.0
        };
        let (swap_total, swap_used) = read_swap_usage();

        MemoryStats {
            total_gb: bytes_to_gb(total),
            used_gb: bytes_to_gb(used),
            used_percent: used_percent as u64,
            swap_total_gb: bytes_to_gb(swap_total),
            swap_used_gb: bytes_to_gb(swap_used),
        }
    }
}

impl Drop for MemoryReader {
    fn drop(&mut self) {
        unsafe {
            if self.host_port != 0 {
                #[allow(deprecated)]
                let task = libc::mach_task_self();
                mach_port_deallocate(task, self.host_port);
            }
        }
    }
}

fn read_swap_usage() -> (u64, u64) {
    let mut swap: libc::xsw_usage = unsafe { mem::zeroed() };
    let mut mib = [libc::CTL_VM, libc::VM_SWAPUSAGE];
    let mut len = mem::size_of::<libc::xsw_usage>();
    let result = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as libc::c_uint,
            &mut swap as *mut _ as *mut c_void,
            &mut len,
            ptr::null_mut(),
            0,
        )
    };
    if result == 0 {
        (swap.xsu_total, swap.xsu_used)
    } else {
        (0, 0)
    }
}

fn read_total_memory() -> Option<u64> {
    let mut value: u64 = 0;
    let mut len = mem::size_of::<u64>();
    let mut mib = [libc::CTL_HW, libc::HW_MEMSIZE];
    let result = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as libc::c_uint,
            &mut value as *mut _ as *mut c_void,
            &mut len,
            ptr::null_mut(),
            0,
        )
    };
    if result == 0 { Some(value) } else { None }
}

fn bytes_to_gb(bytes: u64) -> f64 {
    (bytes as f64) / (1024.0 * 1024.0 * 1024.0)
}

unsafe extern "C" {
    fn mach_port_deallocate(task: mach_port_t, name: mach_port_t) -> c_int;
}
