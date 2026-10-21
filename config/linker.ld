

OUTPUT_FORMAT(elf64-x86-64)
ENTRY(_start)

PHDRS
{
    text    PT_LOAD    FLAGS((1 << 0) | (1 << 2)) ; 
    rodata  PT_LOAD    FLAGS((1 << 2)) ;             
    data    PT_LOAD    FLAGS((1 << 1) | (1 << 2)) ;  
    dynamic PT_DYNAMIC FLAGS((1 << 1) | (1 << 2)) ;
}

SECTIONS
{
    

    . = 0xffffffff80000000;

    



    PROVIDE(__kernel_text_start = .);
    .text : {
        *(.text .text.*)
    } :text
    PROVIDE(__kernel_text_end = .);

    . += CONSTANT(MAXPAGESIZE);

    PROVIDE(__kernel_rodata_start = .);
    .rodata : {
        *(.rodata .rodata.*)

        

        . = ALIGN(8);
        KEEP(*(.requests_start_marker))
        KEEP(*(.requests))
        . = ALIGN(8);
        KEEP(*(.requests_end_marker))
    } :rodata
    PROVIDE(__kernel_rodata_end = .);

    . += CONSTANT(MAXPAGESIZE);

    PROVIDE(__kernel_data_start = .);
    .data : {
        *(.data .data.*)
    } :data

    .dynamic : {
        *(.dynamic)
    } :data :dynamic

    .bss : {
        *(COMMON)
        *(.bss .bss.*)
    } :data
    PROVIDE(__kernel_data_end = .);

    /DISCARD/ : {
        *(.eh_frame)
        *(.note .note.*)
        *(.comment)
    }
}
