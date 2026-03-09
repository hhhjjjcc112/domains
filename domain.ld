OUTPUT_ARCH({{OUTPUT_ARCH}})
ENTRY(main)
BASE_ADDRESS = 0;

SECTIONS
{
    . = BASE_ADDRESS;
    .text :{
        stext = .;
        __executable_start = .;
        *(.text .text.*);
        . = ALIGN(4K);
        etext = .;
       __etext = .;
    }

    .rodata : ALIGN(4096){
        srodata = .;
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
        . = ALIGN(4K);
        erodata = .;
    }

    .data : ALIGN(4096){
        sdata = .;
        *(.data .data.*)
        *(.sdata .sdata.*)
        . = ALIGN(4K);
        edata = .;
    }

    .rela.dyn : ALIGN(4096) {
        __rela_dyn_start = .;
        *(.rela.dyn .rela.dyn*)
        __rela_dyn_end = .;
    }

    .bss :  ALIGN(4096){
        sbss = .;
        *(.bss .bss.*)
        *(.sbss .sbss.*)
        . = ALIGN(4K);
        ebss = .;
    }

    .gcc_except_table : ALIGN(4K){
        kernel_gcc_except_table = .;
        *(.gcc_except_table .gcc_except_table.*)
        kernel_gcc_except_table_end = .;
    }

    .eh_frame_hdr : ALIGN(4K){
        kernel_eh_frame_hdr = .;
        __GNU_EH_FRAME_HDR = .;
        *(.eh_frame_hdr .eh_frame_hdr.*)
        kernel_eh_frame_hdr_end = .;
    }
    .eh_frame : ALIGN(4K){
        kernel_eh_frame = .;
        KEEP(*(.eh_frame .eh_frame.*))
        kernel_eh_frame_end = .;
    }
    . = ALIGN(4K);

 /* If we're on a newer compiler */
    /DISCARD/ :
    {
        *(.interp)
        *(.dynstr)
        *(.hash)
        *(.gnu.hash)
        *(.header)
    } : phdr
    PROVIDE(end = .);
}