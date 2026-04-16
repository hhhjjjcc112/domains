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
        *(.plt .plt.*)
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

    .dynamic : ALIGN(4096) {
        *(.dynamic .dynamic.*)
    }

    .got : ALIGN(4096) {
        *(.got .got.*)
        *(.got.plt .got.plt.*)
    }

    .rela.dyn : ALIGN(4096) {
        __rela_dyn_start = .;
        *(.rela.dyn .rela.dyn*)
        __rela_dyn_end = .;
    }

    .rela.plt : ALIGN(4096) {
        __rela_plt_start = .;
        *(.rela.plt .rela.plt.*)
        __rela_plt_end = .;
    }

    .dynsym : ALIGN(4096) {
        *(.dynsym .dynsym.*)
    }

    .dynstr : ALIGN(4096) {
        *(.dynstr .dynstr.*)
    }

    .gnu.version : ALIGN(4096) {
        *(.gnu.version .gnu.version.*)
    }

    .gnu.version_d : ALIGN(4096) {
        *(.gnu.version_d .gnu.version_d.*)
    }

    .gnu.version_r : ALIGN(4096) {
        *(.gnu.version_r .gnu.version_r.*)
    }

    .hash : ALIGN(4096) {
        *(.hash)
    }

    .gnu.hash : ALIGN(4096) {
        *(.gnu.hash)
    }

    .bss :  ALIGN(4096){
        sbss = .;
        *(.bss .bss.*)
        *(.sbss .sbss.*)
        . = ALIGN(4K);
        ebss = .;
    }

    /* 域镜像不应携带 percpu 运行时数据；这里只保留空段和断言，
       用来在链接期拦截误引入的 percpu 符号。 */
    .percpu : ALIGN(4096) {
        *(.percpu .percpu.*)
    }

    /* TLS 同样不属于域镜像的常规布局，保持为空，避免后续误用。 */
    .tdata : ALIGN(4096) {
        *(.tdata .tdata.*)
    }

    .tbss : ALIGN(4096) {
        *(.tbss .tbss.*)
    }

    ASSERT(SIZEOF(.percpu) == 0, "domain must not contain .percpu")
    ASSERT(SIZEOF(.tdata) == 0, "domain must not contain .tdata")
    ASSERT(SIZEOF(.tbss) == 0, "domain must not contain .tbss")

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
        *(.header)
    } : phdr
    PROVIDE(end = .);
}
