use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

const ELFMAG: [u8; 4] = [0x7f, b'E', b'L', b'F'];
const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;
const EV_CURRENT: u8 = 1;
const ELFOSABI_SYSV: u8 = 0;

const ET_EXEC: u16 = 2;
const EM_X86_64: u16 = 62;

const PT_LOAD: u32 = 1;
const PT_INTERP: u32 = 3;

const PF_X: u32 = 1;
const PF_W: u32 = 2;
const PF_R: u32 = 4;

const SHT_NULL: u32 = 0;
const SHT_PROGBITS: u32 = 1;
const SHT_STRTAB: u32 = 3;
const SHT_NOBITS: u32 = 8;

const SHF_WRITE: u64 = 1;
const SHF_ALLOC: u64 = 2;
const SHF_EXECINSTR: u64 = 4;

const EHDR_SIZE: u64 = 64;
const PHDR_SIZE: u64 = 56;
const SHDR_SIZE: u64 = 64;
const PAGE_SIZE: u64 = 0x1000;

/// A simple, self-contained writer for statically-linked ELF64 x86-64 executables.
///
/// The generated file uses a flat layout where `p_offset = p_vaddr - base_vaddr`,
/// with two `PT_LOAD` segments: one RX segment covering the headers, `.text`,
/// `.rodata` (and `.interp` if present), and one RW segment covering `.data`
/// and `.bss`.
#[derive(Debug, Clone)]
pub struct Elf64Writer {
    pub code: Vec<u8>,
    pub rodata: Vec<u8>,
    pub data: Vec<u8>,
    pub bss_size: u64,
    pub entry_vaddr: u64,
    pub base_vaddr: u64,
    pub interp: Option<String>,
}

impl Elf64Writer {
    /// Create a new writer. `base_vaddr` defaults to `0x400000`.
    pub fn new(
        code: Vec<u8>,
        rodata: Vec<u8>,
        data: Vec<u8>,
        bss_size: u64,
        entry_vaddr: u64,
    ) -> Self {
        Self {
            code,
            rodata,
            data,
            bss_size,
            entry_vaddr,
            base_vaddr: 0x400000,
            interp: None,
        }
    }

    /// Set the virtual base address. `0x400000` is used by default.
    pub fn with_base_vaddr(mut self, base_vaddr: u64) -> Self {
        self.base_vaddr = base_vaddr;
        self
    }

    /// Set the dynamic linker path. When `None`, a static executable is produced.
    pub fn with_interp(mut self, interp: impl Into<String>) -> Self {
        self.interp = Some(interp.into());
        self
    }

    /// Serialize the executable and write it to `path`.
    pub fn write<P: AsRef<Path>>(&self, path: P) -> io::Result<()> {
        let bytes = self.build();
        let mut file = File::create(path)?;
        file.write_all(&bytes)?;
        file.flush()
    }

    fn build(&self) -> Vec<u8> {
        let code_len = self.code.len() as u64;
        let rodata_len = self.rodata.len() as u64;
        let data_len = self.data.len() as u64;
        let interp_len = self
            .interp
            .as_ref()
            .map(|s| s.len() as u64 + 1)
            .unwrap_or(0);

        let has_interp = interp_len > 0;
        let phnum: u16 = if has_interp { 3 } else { 2 };

        // Section name string table.
        let mut shstrtab = ShStrTab::new();
        shstrtab.add(""); // null section
        let text_name = shstrtab.add(".text");
        let rodata_name = shstrtab.add(".rodata");
        let data_name = shstrtab.add(".data");
        let bss_name = shstrtab.add(".bss");
        let shstrtab_name = shstrtab.add(".shstrtab");
        let interp_name = if has_interp { Some(shstrtab.add(".interp")) } else { None };

        // Layout.
        let header_area = EHDR_SIZE + PHDR_SIZE * phnum as u64;
        let interp_offset = if has_interp { header_area } else { 0 };
        let text_offset = header_area + interp_len;
        let rodata_offset = text_offset + code_len;
        let first_content_end = rodata_offset + rodata_len;
        let first_seg_file_size = align_up(first_content_end, PAGE_SIZE);
        let data_offset = first_seg_file_size;
        let bss_offset = data_offset + data_len;
        // .bss is SHT_NOBITS: it occupies no file space, so the section string table
        // is placed right after .data in the file.
        let shstrtab_offset = bss_offset;
        let shstrtab_len = shstrtab.bytes.len() as u64;
        let shdr_offset = align_up(shstrtab_offset + shstrtab_len, 8);

        // Section header table. Order: null, .interp, .text, .rodata, .data, .bss, .shstrtab
        let mut sections: Vec<Section> = Vec::new();
        sections.push(Section::null());

        if has_interp {
            sections.push(Section {
                name_idx: interp_name.unwrap(),
                sh_type: SHT_PROGBITS,
                flags: SHF_ALLOC,
                addr: self.base_vaddr + interp_offset,
                offset: interp_offset,
                size: interp_len,
                addralign: 1,
                ..Section::default()
            });
        }

        sections.push(Section {
            name_idx: text_name,
            sh_type: SHT_PROGBITS,
            flags: SHF_ALLOC | SHF_EXECINSTR,
            addr: self.base_vaddr + text_offset,
            offset: text_offset,
            size: code_len,
            addralign: 16,
            ..Section::default()
        });

        sections.push(Section {
            name_idx: rodata_name,
            sh_type: SHT_PROGBITS,
            flags: SHF_ALLOC,
            addr: self.base_vaddr + rodata_offset,
            offset: rodata_offset,
            size: rodata_len,
            addralign: 8,
            ..Section::default()
        });

        sections.push(Section {
            name_idx: data_name,
            sh_type: SHT_PROGBITS,
            flags: SHF_ALLOC | SHF_WRITE,
            addr: self.base_vaddr + data_offset,
            offset: data_offset,
            size: data_len,
            addralign: 8,
            ..Section::default()
        });

        sections.push(Section {
            name_idx: bss_name,
            sh_type: SHT_NOBITS,
            flags: SHF_ALLOC | SHF_WRITE,
            addr: self.base_vaddr + bss_offset,
            offset: bss_offset,
            size: self.bss_size,
            addralign: 8,
            ..Section::default()
        });

        let shstrtab_section_idx = sections.len();
        sections.push(Section {
            name_idx: shstrtab_name,
            sh_type: SHT_STRTAB,
            flags: 0,
            addr: 0,
            offset: shstrtab_offset,
            size: shstrtab_len,
            addralign: 1,
            ..Section::default()
        });

        let shnum = sections.len() as u16;

        // Program headers.
        let mut phdrs: Vec<ProgramHeader> = Vec::new();
        if has_interp {
            phdrs.push(ProgramHeader {
                p_type: PT_INTERP,
                p_flags: PF_R,
                p_offset: interp_offset,
                p_vaddr: 0,
                p_paddr: 0,
                p_filesz: interp_len,
                p_memsz: 0,
                p_align: 1,
            });
        }
        phdrs.push(ProgramHeader {
            p_type: PT_LOAD,
            p_flags: PF_R | PF_X,
            p_offset: 0,
            p_vaddr: self.base_vaddr,
            p_paddr: self.base_vaddr,
            p_filesz: first_seg_file_size,
            p_memsz: first_seg_file_size,
            p_align: PAGE_SIZE,
        });
        phdrs.push(ProgramHeader {
            p_type: PT_LOAD,
            p_flags: PF_R | PF_W,
            p_offset: data_offset,
            p_vaddr: self.base_vaddr + data_offset,
            p_paddr: self.base_vaddr + data_offset,
            p_filesz: data_len,
            p_memsz: data_len + self.bss_size,
            p_align: PAGE_SIZE,
        });

        let mut out = Vec::with_capacity((shdr_offset + SHDR_SIZE * shnum as u64) as usize);

        // ELF header.
        out.extend_from_slice(&ELFMAG);
        out.push(ELFCLASS64);
        out.push(ELFDATA2LSB);
        out.push(EV_CURRENT);
        out.push(ELFOSABI_SYSV);
        out.extend_from_slice(&[0; 8]); // remaining e_ident bytes
        write_u16(&mut out, ET_EXEC);
        write_u16(&mut out, EM_X86_64);
        write_u32(&mut out, EV_CURRENT as u32);
        write_u64(&mut out, self.entry_vaddr);
        write_u64(&mut out, EHDR_SIZE); // e_phoff
        write_u64(&mut out, shdr_offset); // e_shoff
        write_u32(&mut out, 0); // e_flags
        write_u16(&mut out, EHDR_SIZE as u16); // e_ehsize
        write_u16(&mut out, PHDR_SIZE as u16); // e_phentsize
        write_u16(&mut out, phnum);
        write_u16(&mut out, SHDR_SIZE as u16); // e_shentsize
        write_u16(&mut out, shnum);
        write_u16(&mut out, shstrtab_section_idx as u16); // e_shstrndx

        // Program headers.
        for ph in &phdrs {
            write_u32(&mut out, ph.p_type);
            write_u32(&mut out, ph.p_flags);
            write_u64(&mut out, ph.p_offset);
            write_u64(&mut out, ph.p_vaddr);
            write_u64(&mut out, ph.p_paddr);
            write_u64(&mut out, ph.p_filesz);
            write_u64(&mut out, ph.p_memsz);
            write_u64(&mut out, ph.p_align);
        }

        // Section contents.
        if let Some(interp) = &self.interp {
            out.resize(interp_offset as usize, 0);
            out.extend_from_slice(interp.as_bytes());
            out.push(0);
        }
        out.resize(text_offset as usize, 0);
        out.extend_from_slice(&self.code);
        out.extend_from_slice(&self.rodata);

        // Pad first segment to page boundary.
        out.resize(data_offset as usize, 0);
        out.extend_from_slice(&self.data);

        // .bss takes no file space; the string table follows .data directly.
        out.resize(shstrtab_offset as usize, 0);
        out.extend_from_slice(&shstrtab.bytes);

        // Section headers.
        out.resize(shdr_offset as usize, 0);
        for sec in &sections {
            write_u32(&mut out, sec.name_idx);
            write_u32(&mut out, sec.sh_type);
            write_u64(&mut out, sec.flags);
            write_u64(&mut out, sec.addr);
            write_u64(&mut out, sec.offset);
            write_u64(&mut out, sec.size);
            write_u32(&mut out, sec.link);
            write_u32(&mut out, sec.info);
            write_u64(&mut out, sec.addralign);
            write_u64(&mut out, sec.entsize);
        }

        out
    }
}

#[derive(Debug, Default)]
struct Section {
    name_idx: u32,
    sh_type: u32,
    flags: u64,
    addr: u64,
    offset: u64,
    size: u64,
    link: u32,
    info: u32,
    addralign: u64,
    entsize: u64,
}

impl Section {
    fn null() -> Self {
        Self {
            sh_type: SHT_NULL,
            ..Self::default()
        }
    }
}

#[derive(Debug)]
struct ProgramHeader {
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_paddr: u64,
    p_filesz: u64,
    p_memsz: u64,
    p_align: u64,
}

struct ShStrTab {
    bytes: Vec<u8>,
}

impl ShStrTab {
    fn new() -> Self {
        Self { bytes: vec![0] }
    }

    fn add(&mut self, name: &str) -> u32 {
        let offset = self.bytes.len() as u32;
        self.bytes.extend_from_slice(name.as_bytes());
        self.bytes.push(0);
        offset
    }
}

fn align_up(value: u64, align: u64) -> u64 {
    if align == 0 {
        return value;
    }
    ((value + align - 1) / align) * align
}

fn write_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}
