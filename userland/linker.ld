




ENTRY(_start)

PHDRS
{
    text   PT_LOAD FLAGS(5);   
    rodata PT_LOAD FLAGS(4);   
    data   PT_LOAD FLAGS(6);   
}

SECTIONS
{
    . = 0x400000;

    .text : {
        *(.text .text.*)
    } :text

    . = ALIGN(4096);
    .rodata : {
        *(.rodata .rodata.*)
        KEEP(*(.eonos_app))
    } :rodata

    . = ALIGN(4096);
    .data : {
        *(.data .data.*)
    } :data

    .bss : {
        *(.bss .bss.* COMMON)
    } :data

    /DISCARD/ : {
        *(.note .note.* .comment .eh_frame)
    }
}
