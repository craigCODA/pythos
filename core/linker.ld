ENTRY(pythcore_entry)

PHDRS
{
    rodata PT_LOAD FLAGS(4);
    text PT_LOAD FLAGS(5);
    data PT_LOAD FLAGS(6);
}

SECTIONS
{
    . = 0xFFFFFFFF80000000;

    PROVIDE(__pythcore_rodata_start = .);
    .rodata : ALIGN(4K) {
        *(.rodata .rodata.*)
        *(.eh_frame .eh_frame.*)
    } :rodata
    PROVIDE(__pythcore_rodata_end = .);

    . = ALIGN(4K);
    PROVIDE(__pythcore_text_start = .);
    .text : {
        *(.text .text.*)
    } :text
    PROVIDE(__pythcore_text_end = .);

    . = ALIGN(4K);
    PROVIDE(__pythcore_data_start = .);
    .data : {
        *(.data .data.*)
        *(.got .got.*)
    } :data

    . = ALIGN(4K);
    .bss : {
        *(COMMON)
        *(.bss .bss.*)
    } :data

    /* Task 11: a real, unmapped guard page on each side of the syscall
       kernel stack, so an overflow (or a read below the stack) takes a
       deterministic #PF instead of silently corrupting whatever static
       data the linker would otherwise have placed adjacent to it. These
       ranges are intentionally left out of __pythcore_data_start .. _end's
       normal mapping loop; see map_kernel_segments in memory/virtual.rs. */
    . = ALIGN(4K);
    PROVIDE(__pythcore_syscall_stack_guard_low_start = .);
    . += 4K;
    PROVIDE(__pythcore_syscall_stack_start = .);
    .syscall_stack : ALIGN(4K) {
        *(.syscall_stack)
    } :data
    . = ALIGN(4K);
    PROVIDE(__pythcore_syscall_stack_guard_high_start = .);
    . += 4K;
    PROVIDE(__pythcore_syscall_stack_guard_high_end = .);
    PROVIDE(__pythcore_data_end = .);
}
