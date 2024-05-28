//! Windows OS-specific types and functions.
//!
//! Some useful resources: https://www.geoffchappell.com/studies/windows/km/versions.htm

use std::{
    cmp::max,
    fs::{read, File},
    io::{copy, Write},
    iter::Iterator,
    path::{Path, PathBuf},
};

use crate::{
    arch::{Architecture, ArchitectureOperations},
    Result,
};
use anyhow::{anyhow, bail};
use goblin::pe::PE;
use pdb::PDB;
use raw_cstr::AsRawCstr;
use reqwest::blocking::get;
use simics::{get_attribute, read_phys_memory, sys::access_t};

#[derive(Debug, Clone)]
/// The `KUSER_SHARED_DATA` structure, which is located at 0xFFFFF78000000000 in x86_64
/// Windows and 0xFFDF0000 in x86 Windows. This address is defined as
/// `KI_USER_SHARED_DATA`.
///
/// The used fields of this structure are stable for the versions of Windows noted on each
/// member.
///
/// References:
///
/// - https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntddk/ns-ntddk-kuser_shared_data
/// - https://www.geoffchappell.com/studies/windows/km/ntoskrnl/inc/api/ntexapi_x/kuser_shared_data/index.htm
pub struct KUserSharedData {
    /// The major version number:
    ///
    /// - Windows NT 3.1: 3
    /// - Windows NT 3.5: 3
    /// - Windows NT 3.51: 3
    /// - Windows NT 4.0: 4
    /// - Windows 2000: 5
    /// - Windows XP: 5
    /// - Windows XP 64-bit: 5
    /// - Windows Server 2003: 5
    /// - Windows Server 2003 R2: 5
    /// - Windows Vista: 6
    /// - Windows Server 2008: 6
    /// - Windows 7: 6
    /// - Windows Server 2008 R2: 6
    /// - Windows 8: 6
    /// - Windows Server 2012: 6
    /// - Windows 8.1: 6
    /// - Windows Server 2012 R2: 6
    /// - Windows 10: 10
    /// - Windows Server 2016: 10
    /// - Windows Server 2019: 10
    /// - Windows 11: 10
    /// - Windows Server 2022: 10
    ///
    /// This field is stable for all relevant versions of Windows
    pub nt_major_version: u32,
    /// The minor version number:
    ///
    /// - Windows NT 3.1: 10
    /// - Windows NT 3.5: 50
    /// - Windows NT 3.51: 51
    /// - Windows NT 4.0: 0
    /// - Windows 2000: 0
    /// - Windows XP: 1
    /// - Windows XP 64-bit: 2
    /// - Windows Server 2003: 2
    /// - Windows Server 2003 R2: 2
    /// - Windows Vista: 0
    /// - Windows Server 2008: 0
    /// - Windows 7: 1
    /// - Windows Server 2008 R2: 1
    /// - Windows 8: 2
    /// - Windows Server 2012: 2
    /// - Windows 8.1: 3
    /// - Windows Server 2012 R2: 3
    /// - Windows 10: 0
    /// - Windows Server 2016: 0
    /// - Windows Server 2019: 0
    /// - Windows 11: 0
    /// - Windows Server 2022: 0
    ///
    /// This field is stable for all relevant versions of Windows
    pub nt_minor_version: u32,
    /// The build number (not present before Windows 10):
    ///
    /// - Windows 10 (1507) [Threshold]: 10240
    /// - Windows 10 (1511) [Threshold 2]: 10586
    /// - Windows 10 (1607) [Redstone 1]: 14393
    /// - Windows 10 (1703) [Redstone 2]: 15063
    /// - Windows 10 (1709) [Redstone 3]: 16299
    /// - Windows 10 (1803) [Redstone 4]: 17134
    /// - Windows 10 (1809) [Redstone 5]: 17763
    /// - Windows 10 (1903) [19H1]: 18362
    /// - Windows 10 (1909) [Vanadium]: 18363
    /// - Windows 10 (2004) [Vibranium]: 19041
    /// - Windows 10 (20H2) [Vibranium]: 19042
    /// - Windows 10 (21H1) [Vibranium]: 19043
    /// - Windows 10 (21H2) [Vibranium]: 19044
    /// - Windows 10 (22H2) [Vibranium]: 19045
    /// - Windows 11 (21H2) [Sun Valley]: 22000
    /// - Windows 11 (22H2) [Sun Valley 2]: 22621
    /// - Windows 11 (23H2) []: 22631
    pub nt_build_number: Option<u32>,
}

impl KUserSharedData {
    pub const KUSER_SHARED_DATA_ADDRESS_X86: u64 = 0xFFDF0000;
    pub const KUSER_SHARED_DATA_ADDRESS_X86_64: u64 = 0xFFFFF78000000000;
    pub const NT_BUILD_NUMBER_OFFSET: u64 = 0x26c;
    pub const NT_MAJOR_VERSION_OFFSET: u64 = 0x26c;
    pub const NT_MINOR_VERSION_OFFSET: u64 = 0x270;

    pub fn new(processor: &mut Architecture) -> Result<Self> {
        let kuser_shared_data: u64 =
            if processor.processor_info_v2().get_logical_address_width()? == 64 {
                Self::KUSER_SHARED_DATA_ADDRESS_X86_64
            } else {
                Self::KUSER_SHARED_DATA_ADDRESS_X86
            };

        let nt_major_version = read_phys_memory(
            processor.cpu(),
            processor
                .processor_info_v2()
                .logical_to_physical(
                    kuser_shared_data + Self::NT_MAJOR_VERSION_OFFSET,
                    access_t::Sim_Access_Read,
                )?
                .address,
            std::mem::size_of::<u32>() as i32,
        )? as u32;

        if ![3, 4, 5, 6, 10].contains(&nt_major_version) {
            bail!("Invalid NT major version: {}", nt_major_version);
        }

        let nt_minor_version = read_phys_memory(
            processor.cpu(),
            processor
                .processor_info_v2()
                .logical_to_physical(
                    kuser_shared_data + Self::NT_MINOR_VERSION_OFFSET,
                    access_t::Sim_Access_Read,
                )?
                .address,
            std::mem::size_of::<u32>() as i32,
        )? as u32;

        if ![10, 50, 51, 0, 1, 2, 3].contains(&nt_minor_version) {
            bail!("Invalid NT minor version: {}", nt_minor_version);
        }

        let nt_build_number = if nt_major_version >= 10 {
            Some(read_phys_memory(
                processor.cpu(),
                processor
                    .processor_info_v2()
                    .logical_to_physical(
                        kuser_shared_data + Self::NT_BUILD_NUMBER_OFFSET,
                        access_t::Sim_Access_Read,
                    )?
                    .address,
                std::mem::size_of::<u32>() as i32,
            )? as u32)
        } else {
            None
        };

        Ok(Self {
            nt_major_version,
            nt_minor_version,
            nt_build_number,
        })
    }
}

#[derive(Debug, Clone)]
/// The Kernel Processor Control Region, located at the ia32_gs_base in kernel mode and
/// ia32_kernel_gs_base in user mode.
///
/// This structure is stable for all versions of windows. The members PerfGlobalGroupMask and
/// UserRsp are only present from 5.2 and 6.0 onwards, respectively, but are not used here.
pub struct Kpcr {
    /// The address of the KPCR itself (self-reference). This reference is used to partially
    /// validate the KPCR.
    pub slf: u64,
    /// The address of the current KPRCB.
    pub current_prcb: u64,
    /// The address of the base of the Interrupt Descriptor Table
    pub idt_base: u64,
}

impl Kpcr {
    pub const MSR_IA32_GS_BASE_NAME: &'static str = "ia32_gs_base";
    pub const MSR_IA32_KERNEL_GS_BASE_NAME: &'static str = "ia32_kernel_gs_base";
    pub const KPCR_SELF_OFFSET: u64 = 0x18;
    pub const KPCR_CURRENT_PRCB_OFFSET: u64 = 0x20;
    pub const KPCR_IDT_BASE_OFFSET: u64 = 0x38;

    pub fn new(processor: &mut Architecture) -> Result<Self> {
        let msr_ia32_gs_base_number = processor
            .int_register()
            .get_number(Self::MSR_IA32_GS_BASE_NAME.as_raw_cstr()?)?;
        let msr_ia32_kernel_gs_base_number = processor
            .int_register()
            .get_number(Self::MSR_IA32_KERNEL_GS_BASE_NAME.as_raw_cstr()?)?;
        let ia32_gs_base = processor.int_register().read(msr_ia32_gs_base_number)?;
        let ia32_kernel_gs_base = processor
            .int_register()
            .read(msr_ia32_kernel_gs_base_number)?;

        let kpcr = max(ia32_gs_base, ia32_kernel_gs_base);

        let pointer_size = processor.processor_info_v2().get_logical_address_width()? / 8;

        let slf = read_phys_memory(
            processor.cpu(),
            processor
                .processor_info_v2()
                .logical_to_physical(kpcr + Self::KPCR_SELF_OFFSET, access_t::Sim_Access_Read)?
                .address,
            pointer_size,
        )?;

        if slf != kpcr {
            bail!("KPCR self-reference does not match the KPCR address");
        }

        let current_prcb = read_phys_memory(
            processor.cpu(),
            processor
                .processor_info_v2()
                .logical_to_physical(
                    kpcr + Self::KPCR_CURRENT_PRCB_OFFSET,
                    access_t::Sim_Access_Read,
                )?
                .address,
            pointer_size,
        )?;
        let idt_base = read_phys_memory(
            processor.cpu(),
            processor
                .processor_info_v2()
                .logical_to_physical(kpcr + Self::KPCR_IDT_BASE_OFFSET, access_t::Sim_Access_Read)?
                .address,
            pointer_size,
        )?;

        let sim_idtr_base: u64 = get_attribute(processor.cpu(), "idtr_base")?.try_into()?;

        if idt_base != sim_idtr_base {
            bail!("IDT base does not match the IDTR base");
        }

        Ok(Self {
            slf,
            current_prcb,
            idt_base,
        })
    }
}

#[derive(Debug, Clone)]
/// The Kernel Processor Control Block, which can be located from the [`Kpcr.current_prcb`] field.
///
/// This structure changes significantly across versions of windows, but the members used here are
/// stable for all versions of Windows.
pub struct Kprcb {
    /// The address of the current thread
    pub current_thread: u64,
}

impl Kprcb {
    pub const KPRCB_CURRENT_THREAD_OFFSET: u64 = 0x8;

    pub fn new(processor: &mut Architecture, kpcr: &Kpcr) -> Result<Self> {
        let pointer_size = processor.processor_info_v2().get_logical_address_width()? / 8;

        let current_thread = read_phys_memory(
            processor.cpu(),
            processor
                .processor_info_v2()
                .logical_to_physical(
                    kpcr.current_prcb + Self::KPRCB_CURRENT_THREAD_OFFSET,
                    access_t::Sim_Access_Read,
                )?
                .address,
            pointer_size,
        )?;

        Ok(Self { current_thread })
    }
}

pub trait IDTDescriptor {
    fn offset(&self) -> u64;
    fn selector(&self) -> u16;
    fn gate_type(&self) -> u8;
    fn descriptor_privilege_level(&self) -> u8;
    fn present(&self) -> u8;
}

#[derive(Debug, Clone)]
#[repr(C)]
/// An IDT descriptor entry in the Interrupt Descriptor Table. This structure is defined by the
/// processor architecture and is therefore stable across versions of Windows.
pub struct IDTDescriptor32 {
    /// The offset bits 0..15
    pub offset_1: u16,
    /// A code segment selector in the GDT or LDT
    pub selector: u16,
    /// Unused, set to 0
    pub zero: u8,
    /// Gate type, DPL, and P fields
    pub type_attributes: u8,
    /// The offset bits 16..31
    pub offset_2: u16,
}

impl IDTDescriptor32 {
    pub fn new(processor: &mut Architecture, idt_base: u64, index: u16) -> Result<Self> {
        let idt_descriptor_size = std::mem::size_of::<IDTDescriptor32>() as i32;
        let idt_descriptor_address = idt_base + (index as u64 * idt_descriptor_size as u64);

        let offset_1 = read_phys_memory(
            processor.cpu(),
            processor
                .processor_info_v2()
                .logical_to_physical(idt_descriptor_address, access_t::Sim_Access_Read)?
                .address,
            std::mem::size_of::<u16>() as i32,
        )? as u16;

        let selector = read_phys_memory(
            processor.cpu(),
            processor
                .processor_info_v2()
                .logical_to_physical(
                    idt_descriptor_address + std::mem::size_of::<u16>() as u64,
                    access_t::Sim_Access_Read,
                )?
                .address,
            std::mem::size_of::<u16>() as i32,
        )? as u16;

        let zero = read_phys_memory(
            processor.cpu(),
            processor
                .processor_info_v2()
                .logical_to_physical(
                    idt_descriptor_address + std::mem::size_of::<u16>() as u64 * 2,
                    access_t::Sim_Access_Read,
                )?
                .address,
            std::mem::size_of::<u8>() as i32,
        )? as u8;

        let type_attributes = read_phys_memory(
            processor.cpu(),
            processor
                .processor_info_v2()
                .logical_to_physical(
                    idt_descriptor_address
                        + std::mem::size_of::<u16>() as u64 * 2
                        + std::mem::size_of::<u8>() as u64,
                    access_t::Sim_Access_Read,
                )?
                .address,
            std::mem::size_of::<u8>() as i32,
        )? as u8;

        Ok(Self {
            offset_1,
            selector,
            zero,
            type_attributes,
            offset_2: read_phys_memory(
                processor.cpu(),
                processor
                    .processor_info_v2()
                    .logical_to_physical(
                        idt_descriptor_address
                            + (std::mem::size_of::<u16>() as u64 * 2)
                            + (std::mem::size_of::<u8>() as u64 * 2),
                        access_t::Sim_Access_Read,
                    )?
                    .address,
                std::mem::size_of::<u16>() as i32,
            )? as u16,
        })
    }
}

impl IDTDescriptor for IDTDescriptor32 {
    fn offset(&self) -> u64 {
        (self.offset_2 as u64) << 16 | self.offset_1 as u64
    }

    fn selector(&self) -> u16 {
        self.selector
    }

    fn gate_type(&self) -> u8 {
        self.type_attributes & 0b1111
    }

    fn descriptor_privilege_level(&self) -> u8 {
        (self.type_attributes >> 5) & 0b11
    }

    fn present(&self) -> u8 {
        (self.type_attributes >> 7) & 0b1
    }
}

#[derive(Debug, Clone)]
#[repr(C)]
/// An IDT descriptor entry in the Interrupt Descriptor Table. This structure is defined by the
/// processor architecture and is therefore stable across versions of Windows.
pub struct IDTDescriptor64 {
    /// The offset bits 0..15
    pub offset_1: u16,
    /// A code segment selector in the GDT or LDT
    pub selector: u16,
    /// Unused, set to 0
    pub ist: u8,
    /// Gate type, DPL, and P fields
    pub type_attributes: u8,
    /// The offset bits 16..31
    pub offset_2: u16,
    /// The offset bits 32..63
    pub offset_3: u32,
    /// Reserved, set to 0
    pub reserved: u32,
}

impl IDTDescriptor64 {
    pub fn new(processor: &mut Architecture, idt_base: u64, index: u16) -> Result<Self> {
        let idt_descriptor_size = std::mem::size_of::<IDTDescriptor64>() as i32;
        let idt_descriptor_address = idt_base + (index as u64 * idt_descriptor_size as u64);

        let offset_1 = read_phys_memory(
            processor.cpu(),
            processor
                .processor_info_v2()
                .logical_to_physical(idt_descriptor_address, access_t::Sim_Access_Read)?
                .address,
            std::mem::size_of::<u16>() as i32,
        )? as u16;

        let selector = read_phys_memory(
            processor.cpu(),
            processor
                .processor_info_v2()
                .logical_to_physical(
                    idt_descriptor_address + std::mem::size_of::<u16>() as u64,
                    access_t::Sim_Access_Read,
                )?
                .address,
            std::mem::size_of::<u16>() as i32,
        )? as u16;

        let ist = read_phys_memory(
            processor.cpu(),
            processor
                .processor_info_v2()
                .logical_to_physical(
                    idt_descriptor_address + std::mem::size_of::<u16>() as u64 * 2,
                    access_t::Sim_Access_Read,
                )?
                .address,
            std::mem::size_of::<u8>() as i32,
        )? as u8;

        let type_attributes = read_phys_memory(
            processor.cpu(),
            processor
                .processor_info_v2()
                .logical_to_physical(
                    idt_descriptor_address
                        + std::mem::size_of::<u16>() as u64 * 2
                        + std::mem::size_of::<u8>() as u64,
                    access_t::Sim_Access_Read,
                )?
                .address,
            std::mem::size_of::<u8>() as i32,
        )? as u8;

        Ok(Self {
            offset_1,
            selector,
            ist,
            type_attributes,
            offset_2: read_phys_memory(
                processor.cpu(),
                processor
                    .processor_info_v2()
                    .logical_to_physical(
                        idt_descriptor_address
                            + (std::mem::size_of::<u16>() as u64 * 2)
                            + (std::mem::size_of::<u8>() as u64 * 2),
                        access_t::Sim_Access_Read,
                    )?
                    .address,
                std::mem::size_of::<u16>() as i32,
            )? as u16,
            offset_3: read_phys_memory(
                processor.cpu(),
                processor
                    .processor_info_v2()
                    .logical_to_physical(
                        idt_descriptor_address
                            + (std::mem::size_of::<u16>() as u64 * 2)
                            + (std::mem::size_of::<u8>() as u64 * 2)
                            + std::mem::size_of::<u16>() as u64,
                        access_t::Sim_Access_Read,
                    )?
                    .address,
                std::mem::size_of::<u32>() as i32,
            )? as u32,
            reserved: read_phys_memory(
                processor.cpu(),
                processor
                    .processor_info_v2()
                    .logical_to_physical(
                        idt_descriptor_address
                            + (std::mem::size_of::<u16>() as u64 * 2)
                            + (std::mem::size_of::<u8>() as u64 * 2)
                            + std::mem::size_of::<u16>() as u64
                            + std::mem::size_of::<u32>() as u64,
                        access_t::Sim_Access_Read,
                    )?
                    .address,
                std::mem::size_of::<u32>() as i32,
            )? as u32,
        })
    }
}

impl IDTDescriptor for IDTDescriptor64 {
    fn offset(&self) -> u64 {
        (self.offset_3 as u64) << 32 | (self.offset_2 as u64) << 16 | self.offset_1 as u64
    }

    fn selector(&self) -> u16 {
        self.selector
    }

    fn gate_type(&self) -> u8 {
        self.type_attributes & 0b1111
    }

    fn descriptor_privilege_level(&self) -> u8 {
        (self.type_attributes >> 5) & 0b11
    }

    fn present(&self) -> u8 {
        (self.type_attributes >> 7) & 0b1
    }
}

#[derive(Debug, Clone)]
/// Utilities for parsing PE files/images. Not complete and only implements
/// the functionality needed to support kernel checking and parsing to
/// get debug info.
pub struct PEConstants {}

impl PEConstants {
    pub const DOS_HEADER_OFFSET_E_MAGIC: u64 = 0x0;
    pub const DOS_HEADER_OFFSET_E_LFANEW: u64 = 0x3C;

    pub const DOS_MAGIC: u16 = 0x5A4D;

    pub const NT_HEADER_OFFSET_SIGNATURE: u64 = 0x0;
    pub const NT_HEADER_OFFSET_FILE_HEADER: u64 = 0x4;
    pub const NT_HEADER_OFFSET_OPTIONAL_HEADER: u64 = 0x18;

    pub const NT_SIGNATURE: u16 = 0x4550;

    pub const FILE_HEADER_OFFSET_NUMBER_OF_SECTIONS: u64 = 0x2;
    pub const FILE_HEADER_OFFSET_TIME_DATE_STAMP: u64 = 0x4;
    pub const FILE_HEADER_OFFSET_POINTER_TO_SYMBOL_TABLE: u64 = 0x8;
    pub const FILE_HEADER_OFFSET_NUMBER_OF_SYMBOLS: u64 = 0xC;
    pub const FILE_HEADER_OFFSET_SIZE_OF_OPTIONAL_HEADER: u64 = 0x10;

    pub const OPTIONAL_HEADER_OFFSET_MAGIC: u64 = 0x0;
    pub const OPTIONAL_HEADER_OFFSET_SIZE_OF_CODE: u64 = 0x4;
    pub const OPTIONAL_HEADER_OFFSET_SIZE_OF_INITIALIZED_DATA: u64 = 0x8;
    pub const OPTIONAL_HEADER_OFFSET_SIZE_OF_UNINITIALIZED_DATA: u64 = 0xC;
    pub const OPTIONAL_HEADER_OFFSET_ADDRESS_OF_ENTRY_POINT: u64 = 0x10;
    pub const OPTIONAL_HEADER_OFFSET_BASE_OF_CODE: u64 = 0x14;
    pub const OPTIONAL_HEADER_OFFSET_IMAGE_BASE: u64 = 0x18;
    pub const OPTIONAL_HEADER_OFFSET_SIZE_OF_IMAGE: u64 = 0x38;
    pub const OPTIONAL_HEADER_OFFSET_SIZE_OF_HEADERS: u64 = 0x3C;
    pub const OPTIONAL_HEADER_OFFSET_DATA_DIRECTORY: u64 = 0x70;

    pub const OPTIONAL_HEADER_SIGNATURE_PE32: u16 = 0x10B;
    pub const OPTIONAL_HEADER_SIGNATURE_PE32_PLUS: u16 = 0x20B;

    pub const OPTIONAL_HEADER_SIZE: u64 = 0xf0;

    pub const DATA_DIRECTORY_SIZE: u64 = 0x8;

    pub const DATA_DIRECTORY_INDEX_EXPORT: u64 = 0x0;
    pub const DATA_DIRECTORY_INDEX_IMPORT: u64 = 0x1;
    pub const DATA_DIRECTORY_INDEX_RESOURCE: u64 = 0x2;
    pub const DATA_DIRECTORY_INDEX_EXCEPTION: u64 = 0x3;
    pub const DATA_DIRECTORY_INDEX_CERTIFICATE: u64 = 0x4;
    pub const DATA_DIRECTORY_INDEX_RELOCATION: u64 = 0x5;
    pub const DATA_DIRECTORY_INDEX_DEBUG: u64 = 0x6;
    pub const DATA_DIRECTORY_INDEX_ARCHITECTURE: u64 = 0x7;
    pub const DATA_DIRECTORY_INDEX_GLOBAL_PTR: u64 = 0x8;
    pub const DATA_DIRECTORY_INDEX_TLS: u64 = 0x9;
    pub const DATA_DIRECTORY_INDEX_LOAD_CONFIG: u64 = 0xA;
    pub const DATA_DIRECTORY_INDEX_BOUND_IMPORT: u64 = 0xB;
    pub const DATA_DIRECTORY_INDEX_IAT: u64 = 0xC;
    pub const DATA_DIRECTORY_INDEX_DELAY_IMPORT: u64 = 0xD;
    pub const DATA_DIRECTORY_INDEX_COM_DESCRIPTOR: u64 = 0xE;

    pub const DATA_DIRECTORY_OFFSET_VIRTUAL_ADDRESS: u64 = 0x0;
    pub const DATA_DIRECTORY_OFFSET_SIZE: u64 = 0x4;

    pub const DEBUG_DIRECTORY_SIZE: u64 = 0x1c;

    pub const DEBUG_DIRECTORY_OFFSET_TIME_DATE_STAMP: u64 = 0x4;
    pub const DEBUG_DIRECTORY_OFFSET_MAJOR_VERSION: u64 = 0x8;
    pub const DEBUG_DIRECTORY_OFFSET_MINOR_VERSION: u64 = 0xA;
    pub const DEBUG_DIRECTORY_OFFSET_TYPE: u64 = 0xC;
    pub const DEBUG_DIRECTORY_OFFSET_SIZE_OF_DATA: u64 = 0x10;
    pub const DEBUG_DIRECTORY_OFFSET_ADDRESS_OF_RAW_DATA: u64 = 0x14;
    pub const DEBUG_DIRECTORY_OFFSET_POINTER_TO_RAW_DATA: u64 = 0x18;

    pub const DEBUG_DIRECTORY_TYPE_UNKNOWN: u32 = 0x0;
    pub const DEBUG_DIRECTORY_TYPE_COFF: u32 = 0x1;
    pub const DEBUG_DIRECTORY_TYPE_CODEVIEW: u32 = 0x2;
    pub const DEBUG_DIRECTORY_TYPE_FPO: u32 = 0x3;
    pub const DEBUG_DIRECTORY_TYPE_MISC: u32 = 0x4;
    pub const DEBUG_DIRECTORY_TYPE_EXCEPTION: u32 = 0x5;
    pub const DEBUG_DIRECTORY_TYPE_FIXUP: u32 = 0x6;
    pub const DEBUG_DIRECTORY_TYPE_BORLAND: u32 = 0x9;

    pub const EXPORT_DIRECTORY_TABLE_OFFSET_NAME_RVA: u64 = 0xc;
}

#[derive(Debug, Clone)]
pub struct Kernel {
    base: u64,
}

impl Kernel {
    pub const PAGE_SIZE: u64 = 0x1000;
    pub const HUGE_PAGE_SIZE: u64 = 0x200000;

    pub fn page_is_kernel(processor: &mut Architecture, page: u64) -> Result<bool> {
        // Check DOS magic
        let dos_magic = read_phys_memory(
            processor.cpu(),
            processor
                .processor_info_v2()
                .logical_to_physical(page, access_t::Sim_Access_Read)?
                .address,
            2,
        )? as u16;

        if dos_magic != PEConstants::DOS_MAGIC {
            return Ok(false);
        }

        // Check NT signature
        let nt_header_offset = read_phys_memory(
            processor.cpu(),
            processor
                .processor_info_v2()
                .logical_to_physical(
                    page + PEConstants::DOS_HEADER_OFFSET_E_LFANEW,
                    access_t::Sim_Access_Read,
                )?
                .address,
            4,
        )? as u64;

        let nt_headers_address = page + nt_header_offset;

        let nt_signature = read_phys_memory(
            processor.cpu(),
            processor
                .processor_info_v2()
                .logical_to_physical(nt_headers_address, access_t::Sim_Access_Read)?
                .address,
            2,
        )? as u16;

        if nt_signature != PEConstants::NT_SIGNATURE {
            return Ok(false);
        }

        // Check optional header size
        let optional_header_size = read_phys_memory(
            processor.cpu(),
            processor
                .processor_info_v2()
                .logical_to_physical(
                    nt_headers_address
                        + PEConstants::NT_HEADER_OFFSET_FILE_HEADER
                        + PEConstants::FILE_HEADER_OFFSET_SIZE_OF_OPTIONAL_HEADER,
                    access_t::Sim_Access_Read,
                )?
                .address,
            2,
        )? as u16;

        if optional_header_size as u64 != PEConstants::OPTIONAL_HEADER_SIZE {
            return Ok(false);
        }

        // Check optional header magic
        let optional_header_magic = read_phys_memory(
            processor.cpu(),
            processor
                .processor_info_v2()
                .logical_to_physical(
                    nt_headers_address
                        + PEConstants::NT_HEADER_OFFSET_OPTIONAL_HEADER
                        + PEConstants::OPTIONAL_HEADER_OFFSET_MAGIC,
                    access_t::Sim_Access_Read,
                )?
                .address,
            2,
        )? as u16;

        if ![
            PEConstants::OPTIONAL_HEADER_SIGNATURE_PE32,
            PEConstants::OPTIONAL_HEADER_SIGNATURE_PE32_PLUS,
        ]
        .contains(&optional_header_magic)
        {
            return Ok(false);
        }

        let image_size = read_phys_memory(
            processor.cpu(),
            processor
                .processor_info_v2()
                .logical_to_physical(
                    nt_headers_address
                        + PEConstants::NT_HEADER_OFFSET_OPTIONAL_HEADER
                        + PEConstants::OPTIONAL_HEADER_OFFSET_SIZE_OF_IMAGE,
                    access_t::Sim_Access_Read,
                )?
                .address,
            4,
        )?;

        let export_header_offset = read_phys_memory(
            processor.cpu(),
            processor
                .processor_info_v2()
                .logical_to_physical(
                    nt_headers_address
                        + PEConstants::NT_HEADER_OFFSET_OPTIONAL_HEADER
                        + PEConstants::OPTIONAL_HEADER_OFFSET_DATA_DIRECTORY
                        + PEConstants::DATA_DIRECTORY_INDEX_EXPORT
                        + PEConstants::DATA_DIRECTORY_OFFSET_VIRTUAL_ADDRESS,
                    access_t::Sim_Access_Read,
                )?
                .address,
            4,
        )?;

        if export_header_offset == 0 {
            return Ok(false);
        }

        let export_header_size = read_phys_memory(
            processor.cpu(),
            processor
                .processor_info_v2()
                .logical_to_physical(
                    nt_headers_address
                        + PEConstants::NT_HEADER_OFFSET_OPTIONAL_HEADER
                        + PEConstants::OPTIONAL_HEADER_OFFSET_DATA_DIRECTORY
                        + PEConstants::DATA_DIRECTORY_INDEX_EXPORT
                        + PEConstants::DATA_DIRECTORY_OFFSET_SIZE,
                    access_t::Sim_Access_Read,
                )?
                .address,
            4,
        )?;

        if export_header_size == 0 {
            return Ok(false);
        }

        if export_header_offset + export_header_size > image_size {
            return Ok(false);
        }

        let export_directory_table_name_offset = read_phys_memory(
            processor.cpu(),
            processor
                .processor_info_v2()
                .logical_to_physical(
                    page + export_header_offset
                        + PEConstants::EXPORT_DIRECTORY_TABLE_OFFSET_NAME_RVA,
                    access_t::Sim_Access_Read,
                )?
                .address,
            4,
        )?;

        let mut export_table_name_address = page + export_directory_table_name_offset;
        let mut export_table_name = "".to_string();

        loop {
            let character = read_phys_memory(
                processor.cpu(),
                processor
                    .processor_info_v2()
                    .logical_to_physical(export_table_name_address, access_t::Sim_Access_Read)?
                    .address,
                1,
            )? as u8;

            if character == 0 {
                break;
            }

            export_table_name.push(character as char);

            export_table_name_address += 1;
        }

        if export_table_name != "ntoskrnl.exe" {
            return Ok(false);
        }

        Ok(true)
    }

    pub fn pe_guid(processor: &mut Architecture, base: u64) -> Result<String> {
        // Get the PE guid which is {FILE_HEADER_TIME_DATE_STAMP}{SIZE_OF_IMAGE}
        let nt_headers_address = base
            + read_phys_memory(
                processor.cpu(),
                processor
                    .processor_info_v2()
                    .logical_to_physical(
                        base + PEConstants::DOS_HEADER_OFFSET_E_LFANEW,
                        access_t::Sim_Access_Read,
                    )?
                    .address,
                4,
            )? as u64;

        let time_date_stamp = read_phys_memory(
            processor.cpu(),
            processor
                .processor_info_v2()
                .logical_to_physical(
                    nt_headers_address
                        + PEConstants::NT_HEADER_OFFSET_FILE_HEADER
                        + PEConstants::FILE_HEADER_OFFSET_TIME_DATE_STAMP,
                    access_t::Sim_Access_Read,
                )?
                .address,
            4,
        )?;

        let size_of_image = read_phys_memory(
            processor.cpu(),
            processor
                .processor_info_v2()
                .logical_to_physical(
                    nt_headers_address
                        + PEConstants::NT_HEADER_OFFSET_OPTIONAL_HEADER
                        + PEConstants::OPTIONAL_HEADER_OFFSET_SIZE_OF_IMAGE,
                    access_t::Sim_Access_Read,
                )?
                .address,
            4,
        )?;

        Ok(format!("{:08X}{:05X}", time_date_stamp, size_of_image))
    }

    pub fn find(processor: &mut Architecture) -> Result<Self> {
        let kuser_shared_data = KUserSharedData::new(processor)?;
        let step = if kuser_shared_data.nt_major_version >= 10
            && kuser_shared_data
                .nt_build_number
                .is_some_and(|x| x >= 19000)
        {
            Kernel::HUGE_PAGE_SIZE
        } else {
            Kernel::PAGE_SIZE
        };

        let kpcr = Kpcr::new(processor)?;
        let idtr_entry0 = IDTDescriptor32::new(processor, kpcr.idt_base, 0)?;
        let mut scan_address = idtr_entry0.offset() & !(step - 1);
        let scan_stop_address = idtr_entry0.offset() & !(0x1000000000000 - 1);

        while scan_address >= scan_stop_address {
            if Kernel::page_is_kernel(processor, scan_address)? {
                return Ok(Self { base: scan_address });
            }

            scan_address -= step;
        }

        bail!("Kernel not found")
    }
}

#[derive(Debug, Clone)]
#[repr(C)]
pub struct Guid {
    pub data0: u32,
    pub data1: u16,
    pub data2: u16,
    pub data3: u64,
}

#[derive(Debug, Clone)]
pub struct CvInfoPdb70 {
    pub cv_signature: u32,
    pub signature: Guid,
    pub age: u8,
    pub file_name: String,
}

impl CvInfoPdb70 {
    pub fn new(processor: &mut Architecture, header_address: u64) -> Result<Self> {
        let cv_signature = read_phys_memory(
            processor.cpu(),
            processor
                .processor_info_v2()
                .logical_to_physical(header_address, access_t::Sim_Access_Read)?
                .address,
            4,
        )? as u32;

        if cv_signature != 0x53445352 {
            bail!("Invalid CV signature: {}", cv_signature);
        }

        let data0 = read_phys_memory(
            processor.cpu(),
            processor
                .processor_info_v2()
                .logical_to_physical(header_address + 4, access_t::Sim_Access_Read)?
                .address,
            4,
        )? as u32;

        let data1 = read_phys_memory(
            processor.cpu(),
            processor
                .processor_info_v2()
                .logical_to_physical(header_address + 8, access_t::Sim_Access_Read)?
                .address,
            2,
        )? as u16;

        let data2 = read_phys_memory(
            processor.cpu(),
            processor
                .processor_info_v2()
                .logical_to_physical(header_address + 10, access_t::Sim_Access_Read)?
                .address,
            2,
        )? as u16;

        let mut data3 = read_phys_memory(
            processor.cpu(),
            processor
                .processor_info_v2()
                .logical_to_physical(header_address + 12, access_t::Sim_Access_Read)?
                .address,
            8,
        )?;

        // Reverse the byte order of data3
        data3 = u64::from_be_bytes(data3.to_le_bytes());

        let signature = Guid {
            data0,
            data1,
            data2,
            data3,
        };

        let age = read_phys_memory(
            processor.cpu(),
            processor
                .processor_info_v2()
                .logical_to_physical(header_address + 20, access_t::Sim_Access_Read)?
                .address,
            1,
        )? as u8;

        let mut file_name = "".to_string();
        let mut file_name_address = header_address + 21;

        loop {
            let character = read_phys_memory(
                processor.cpu(),
                processor
                    .processor_info_v2()
                    .logical_to_physical(file_name_address, access_t::Sim_Access_Read)?
                    .address,
                1,
            )? as u8;

            if character == 0 {
                break;
            }

            file_name.push(character as char);

            file_name_address += 1;
        }

        Ok(Self {
            cv_signature,
            signature,
            age,
            file_name,
        })
    }

    pub fn guid(&self) -> String {
        format!(
            "{:08X}{:04X}{:04X}{:016X}{:01X}",
            self.signature.data0,
            self.signature.data1,
            self.signature.data2,
            self.signature.data3,
            self.age
        )
    }

    pub fn file_name(&self) -> String {
        self.file_name.clone()
    }
}

#[derive(Debug, Clone)]
pub struct Export {
    name: Option<String>,
    offset: Option<usize>,
    rva: usize,
    size: usize,
}

impl From<goblin::pe::export::Export<'_>> for Export {
    fn from(export: goblin::pe::export::Export) -> Self {
        Self {
            name: export.name.map(|s| s.to_string()),
            offset: export.offset,
            rva: export.rva,
            size: export.size,
        }
    }
}

impl From<&goblin::pe::export::Export<'_>> for Export {
    fn from(export: &goblin::pe::export::Export) -> Self {
        Self {
            name: export.name.map(|s| s.to_string()),
            offset: export.offset,
            rva: export.rva,
            size: export.size,
        }
    }
}

#[derive(Debug)]
pub struct KernelDebugInfo<'a> {
    kernel: Kernel,
    pdb: PDB<'a, File>,
    exports: Vec<Export>,
}

impl<'a> KernelDebugInfo<'a> {
    pub fn new<P>(processor: &mut Architecture, download_directory: P) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let kernel = Kernel::find(processor)?;

        let nt_headers_offset = read_phys_memory(
            processor.cpu(),
            processor
                .processor_info_v2()
                .logical_to_physical(
                    kernel.base + PEConstants::DOS_HEADER_OFFSET_E_LFANEW,
                    access_t::Sim_Access_Read,
                )?
                .address,
            4,
        )? as u64;

        let nt_headers_address = kernel.base + nt_headers_offset;
        let optional_headers_address =
            nt_headers_address + PEConstants::NT_HEADER_OFFSET_OPTIONAL_HEADER;
        let data_directory_address =
            optional_headers_address + PEConstants::OPTIONAL_HEADER_OFFSET_DATA_DIRECTORY;
        let debug_data_directory = data_directory_address
            + PEConstants::DATA_DIRECTORY_INDEX_DEBUG * PEConstants::DATA_DIRECTORY_SIZE;

        let debug_data_directory_vaddr = read_phys_memory(
            processor.cpu(),
            processor
                .processor_info_v2()
                .logical_to_physical(
                    debug_data_directory + PEConstants::DATA_DIRECTORY_OFFSET_VIRTUAL_ADDRESS,
                    access_t::Sim_Access_Read,
                )?
                .address,
            4,
        )?;

        let debug_data_directory_size = read_phys_memory(
            processor.cpu(),
            processor
                .processor_info_v2()
                .logical_to_physical(
                    debug_data_directory + PEConstants::DATA_DIRECTORY_OFFSET_SIZE,
                    access_t::Sim_Access_Read,
                )?
                .address,
            4,
        )?;

        let debug_data_directory_address = kernel.base + debug_data_directory_vaddr;

        let mut pdb_offset = 0;
        let mut pdb_size = 0;

        for debug_directory_address in (debug_data_directory_address
            ..debug_data_directory_address + debug_data_directory_size)
            .step_by(PEConstants::DEBUG_DIRECTORY_SIZE as usize)
        {
            let debug_directory_type = read_phys_memory(
                processor.cpu(),
                processor
                    .processor_info_v2()
                    .logical_to_physical(
                        debug_directory_address + PEConstants::DEBUG_DIRECTORY_OFFSET_TYPE,
                        access_t::Sim_Access_Read,
                    )?
                    .address,
                4,
            )? as u32;

            if debug_directory_type != PEConstants::DEBUG_DIRECTORY_TYPE_CODEVIEW {
                continue;
            }

            pdb_offset = read_phys_memory(
                processor.cpu(),
                processor
                    .processor_info_v2()
                    .logical_to_physical(
                        debug_directory_address
                            + PEConstants::DEBUG_DIRECTORY_OFFSET_POINTER_TO_RAW_DATA,
                        access_t::Sim_Access_Read,
                    )?
                    .address,
                4,
            )?;

            pdb_size = read_phys_memory(
                processor.cpu(),
                processor
                    .processor_info_v2()
                    .logical_to_physical(
                        debug_directory_address + PEConstants::DEBUG_DIRECTORY_OFFSET_SIZE_OF_DATA,
                        access_t::Sim_Access_Read,
                    )?
                    .address,
                4,
            )?;

            break;
        }

        if pdb_offset == 0 || pdb_size == 0 {
            bail!("PDB not found");
        }

        let pdb_address = kernel.base + pdb_offset;

        let cv_info_pdb70 = CvInfoPdb70::new(processor, pdb_address)?;

        // Download kernel PDB file
        let pdb_url = format!(
            "https://msdl.microsoft.com/download/symbols/{}/{}/{}",
            cv_info_pdb70.file_name(),
            cv_info_pdb70.guid(),
            cv_info_pdb70.file_name()
        );

        let exe_url = format!(
            "https://msdl.microsoft.com/download/symbols/{}/{}/{}",
            "ntoskrnl.exe",
            Kernel::pe_guid(processor, kernel.base)?,
            "ntoskrnl.exe"
        );

        let pdb_path = download_directory
            .as_ref()
            .join(format!("{}.pdb", cv_info_pdb70.guid()));

        if !pdb_path.exists() {
            let response = get(&pdb_url)?.error_for_status()?;

            let mut file = File::create(&pdb_path)?;
            copy(&mut response.bytes()?.as_ref(), &mut file)?;
            file.flush()?;
        }

        let pdb_file = File::open(&pdb_path)?;

        let pdb = PDB::open(pdb_file)?;

        // Download kernel PE file
        let exe_path = download_directory
            .as_ref()
            .join(format!("{}.exe", Kernel::pe_guid(processor, kernel.base)?));

        if !exe_path.exists() {
            let response = get(&exe_url)?.error_for_status()?;

            let mut file = File::create(&exe_path)?;
            copy(&mut response.bytes()?.as_ref(), &mut file)?;
            file.flush()?;
        }

        let exe_file_contents = read(&exe_path)?;

        let exe = PE::parse(&exe_file_contents)?;

        let exports = exe.exports.iter().map(Export::from).collect();

        Ok(Self {
            kernel: kernel.clone(),
            pdb,
            exports,
        })
    }
}
