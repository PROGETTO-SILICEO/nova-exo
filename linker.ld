ENTRY(_start)

PHDRS {
    limine  PT_LOAD;
    text    PT_LOAD;
    rodata  PT_LOAD;
    data    PT_LOAD;
    bss     PT_LOAD;
}

SECTIONS {
    . = 0xffffffff80000000;

    .limine_reqs : {
        *(.limine_reqs)
    } :limine

    . = ALIGN(0x1000);

    .text : {
        *(.text.startup)
        *(.text .text.*)
        *(.text.*)
    } :text

    . = ALIGN(0x1000);

    .rodata : {
        *(.rodata .rodata.*)
        *(.got)
        *(.got.*)
    } :rodata

    . = ALIGN(0x1000);

    .data : {
        *(.data .data.*)
    } :data

    . = ALIGN(0x1000);

    .bss : {
        *(.bss .bss.*)
        *(COMMON)
    } :bss

    /DISCARD/ : {
        *(.rela.*)
        *(.rel.*)
        *(.plt)
        *(.iplt)
        *(.note.*)
        *(.gnu.hash)
        *(.hash)
        *(.dynamic)
        *(.dynsym)
        *(.dynstr)
        *(.gnu.version_r)
    }
}
