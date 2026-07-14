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

    __pythcore_rodata_start = .;
    .rodata : ALIGN(4K) {
        *(.rodata .rodata.*)
        *(.eh_frame .eh_frame.*)
    } :rodata
    __pythcore_rodata_end = .;

    . = ALIGN(4K);
    __pythcore_text_start = .;
    .text : {
        *(.text .text.*)
    } :text
    __pythcore_text_end = .;

    . = ALIGN(4K);
    __pythcore_data_start = .;
    .data : {
        *(.data .data.*)
        *(.got .got.*)
    } :data

    . = ALIGN(4K);
    .bss : {
        *(COMMON)
        *(.bss .bss.*)
    } :data
    __pythcore_data_end = .;
}
