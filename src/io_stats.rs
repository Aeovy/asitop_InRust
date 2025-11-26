use core_foundation_sys::{
    base::{Boolean, CFAllocatorRef, CFRelease, CFTypeRef},
    dictionary::{CFDictionaryRef, CFMutableDictionaryRef},
    number::{CFNumberRef, CFNumberType, kCFNumberSInt64Type},
    string::{CFStringEncoding, CFStringRef, kCFStringEncodingUTF8},
};
use libc::{
    self, AF_LINK, IFF_LOOPBACK, IFF_UP, KERN_SUCCESS, c_char, c_void, freeifaddrs, getifaddrs,
    if_data, ifaddrs, mach_port_t,
};
use std::{ffi::CString, ptr, time::{Duration, Instant}};

const MIN_SAMPLE_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, Default)]
pub struct IoStats {
    pub net_in_mbps: f32,
    pub net_out_mbps: f32,
    pub disk_read_mbps: f32,
    pub disk_write_mbps: f32,
}

pub struct IoSampler {
    last_net: Option<(u64, u64)>,
    last_disk: Option<(u64, u64)>,
    last_instant: Option<Instant>,
    current: IoStats,
}

impl IoSampler {
    pub fn new() -> Self {
        Self {
            last_net: None,
            last_disk: None,
            last_instant: None,
            current: IoStats::default(),
        }
    }

    pub fn sample(&mut self) -> IoStats {
        let now = Instant::now();

        // Skip sampling if not enough time has passed
        if let Some(last) = self.last_instant {
            if now.duration_since(last) < MIN_SAMPLE_INTERVAL {
                return self.current;
            }
        }

        let net_totals = read_network_counters();
        let disk_totals = read_disk_counters();

        if self.last_instant.is_none() {
            self.last_instant = Some(now);
            self.last_net = net_totals;
            self.last_disk = disk_totals;
            self.current = IoStats::default();
            return self.current;
        }

        let delta = now
            .duration_since(self.last_instant.unwrap_or(now))
            .as_secs_f64()
            .max(0.001);

        if let Some((in_bytes, out_bytes)) = net_totals {
            if let Some((prev_in, prev_out)) = self.last_net {
                self.current.net_in_mbps = rate_from_delta(in_bytes, prev_in, delta);
                self.current.net_out_mbps = rate_from_delta(out_bytes, prev_out, delta);
            }
            self.last_net = Some((in_bytes, out_bytes));
        }

        if let Some((read_bytes, write_bytes)) = disk_totals {
            if let Some((prev_read, prev_write)) = self.last_disk {
                self.current.disk_read_mbps = rate_from_delta(read_bytes, prev_read, delta);
                self.current.disk_write_mbps = rate_from_delta(write_bytes, prev_write, delta);
            }
            self.last_disk = Some((read_bytes, write_bytes));
        }

        self.last_instant = Some(now);
        self.current
    }
}

fn rate_from_delta(current: u64, previous: u64, delta_secs: f64) -> f32 {
    if current <= previous || delta_secs <= 0.0 {
        0.0
    } else {
        let diff = current - previous;
        (diff as f64 / delta_secs / (1024.0 * 1024.0)) as f32
    }
}

fn read_network_counters() -> Option<(u64, u64)> {
    // SAFETY: We use getifaddrs/freeifaddrs correctly:
    // 1. ifap is initialized to null before getifaddrs
    // 2. We check both return value and null pointer
    // 3. We always call freeifaddrs before returning
    // 4. All pointer dereferences are guarded by null checks
    unsafe {
        let mut ifap: *mut ifaddrs = ptr::null_mut();
        if getifaddrs(&mut ifap) != 0 {
            return None;
        }
        if ifap.is_null() {
            return None;
        }
        
        let mut total_in = 0u64;
        let mut total_out = 0u64;
        let mut cursor = ifap;
        
        // Limit iterations to prevent infinite loops from corrupted data
        const MAX_INTERFACES: usize = 1000;
        let mut iterations = 0;
        
        while !cursor.is_null() && iterations < MAX_INTERFACES {
            iterations += 1;
            let iface = &*cursor;
            
            // Validate ifa_addr before dereferencing
            if !iface.ifa_addr.is_null() {
                let sa_family = (*iface.ifa_addr).sa_family as i32;
                if sa_family == AF_LINK {
                    let flags = iface.ifa_flags as i32;
                    if (flags & IFF_UP) != 0 && (flags & IFF_LOOPBACK) == 0 {
                        // Validate ifa_data pointer before use
                        let data_ptr = iface.ifa_data as *const if_data;
                        if !data_ptr.is_null() {
                            // Use as_ref for safe optional dereference
                            if let Some(data) = data_ptr.as_ref() {
                                total_in = total_in.saturating_add(data.ifi_ibytes as u64);
                                total_out = total_out.saturating_add(data.ifi_obytes as u64);
                            }
                        }
                    }
                }
            }
            cursor = iface.ifa_next;
        }
        
        freeifaddrs(ifap);
        Some((total_in, total_out))
    }
}

fn read_disk_counters() -> Option<(u64, u64)> {
    unsafe {
        let matching = IOServiceMatching(b"IOBlockStorageDriver\0".as_ptr() as *const c_char);
        if matching.is_null() {
            return None;
        }
        let mut iterator: io_iterator_t = 0;
        let result = IOServiceGetMatchingServices(0, matching, &mut iterator);
        if result != KERN_SUCCESS {
            if iterator != 0 {
                IOObjectRelease(iterator);
            }
            return None;
        }
        let mut total_read = 0u64;
        let mut total_write = 0u64;
        loop {
            let entry = IOIteratorNext(iterator);
            if entry == 0 {
                break;
            }
            if let Some((read, write)) = read_entry_bytes(entry) {
                total_read = total_read.saturating_add(read);
                total_write = total_write.saturating_add(write);
            }
            IOObjectRelease(entry);
        }
        if iterator != 0 {
            IOObjectRelease(iterator);
        }
        Some((total_read, total_write))
    }
}

fn read_entry_bytes(entry: io_registry_entry_t) -> Option<(u64, u64)> {
    unsafe {
        let mut properties: CFMutableDictionaryRef = ptr::null_mut();
        let result = IORegistryEntryCreateCFProperties(entry, &mut properties, ptr::null(), 0);
        if result != KERN_SUCCESS || properties.is_null() {
            return None;
        }
        let parsed = (|| {
            let stats_dict = get_dict_value(properties as CFDictionaryRef, "Statistics")?;
            let bytes_read = get_number(stats_dict, "Bytes (Read)")?;
            let bytes_write = get_number(stats_dict, "Bytes (Write)")?;
            Some((bytes_read, bytes_write))
        })();
        CFRelease(properties as CFTypeRef);
        parsed
    }
}

fn get_dict_value(dict: CFDictionaryRef, key: &str) -> Option<CFDictionaryRef> {
    let cf_key = cf_string(key)?;
    let mut value: *const c_void = ptr::null();
    let success =
        unsafe { CFDictionaryGetValueIfPresent(dict, cf_key as *const c_void, &mut value) };
    unsafe {
        CFRelease(cf_key as CFTypeRef);
    }
    if success == 0 || value.is_null() {
        None
    } else {
        Some(value as CFDictionaryRef)
    }
}

fn get_number(dict: CFDictionaryRef, key: &str) -> Option<u64> {
    let cf_key = cf_string(key)?;
    let mut value: *const c_void = ptr::null();
    let success =
        unsafe { CFDictionaryGetValueIfPresent(dict, cf_key as *const c_void, &mut value) };
    unsafe {
        CFRelease(cf_key as CFTypeRef);
    }
    if success == 0 || value.is_null() {
        return None;
    }
    let mut raw: i64 = 0;
    let ok = unsafe {
        CFNumberGetValue(
            value as CFNumberRef,
            kCFNumberSInt64Type as CFNumberType,
            &mut raw as *mut _ as *mut c_void,
        )
    };
    if ok == 0 {
        return None;
    }
    Some(raw.max(0) as u64)
}

fn cf_string(value: &str) -> Option<CFStringRef> {
    let cstring = CString::new(value).ok()?;
    let cf = unsafe {
        CFStringCreateWithCString(
            ptr::null(),
            cstring.as_ptr(),
            kCFStringEncodingUTF8 as CFStringEncoding,
        )
    };
    if cf.is_null() { None } else { Some(cf) }
}

#[allow(non_camel_case_types)]
type io_object_t = mach_port_t;
#[allow(non_camel_case_types)]
type io_iterator_t = io_object_t;
#[allow(non_camel_case_types)]
type io_registry_entry_t = io_object_t;

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOServiceMatching(name: *const c_char) -> CFMutableDictionaryRef;
    fn IOServiceGetMatchingServices(
        master_port: mach_port_t,
        matching: CFMutableDictionaryRef,
        existing: *mut io_iterator_t,
    ) -> libc::kern_return_t;
    fn IOIteratorNext(iterator: io_iterator_t) -> io_object_t;
    fn IOObjectRelease(object: io_object_t) -> libc::kern_return_t;
    fn IORegistryEntryCreateCFProperties(
        entry: io_registry_entry_t,
        properties: *mut CFMutableDictionaryRef,
        allocator: CFAllocatorRef,
        options: u32,
    ) -> libc::kern_return_t;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFStringCreateWithCString(
        alloc: CFAllocatorRef,
        c_str: *const c_char,
        encoding: CFStringEncoding,
    ) -> CFStringRef;
    fn CFDictionaryGetValueIfPresent(
        dict: CFDictionaryRef,
        key: *const c_void,
        value: *mut *const c_void,
    ) -> Boolean;
    fn CFNumberGetValue(
        number: CFNumberRef,
        the_type: CFNumberType,
        value_ptr: *mut c_void,
    ) -> Boolean;
}
