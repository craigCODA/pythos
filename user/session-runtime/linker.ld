/* Retained ring-3 session runtime ELF. Keep explicit PT_LOAD-only program
 * headers so the existing user ELF verifier can enforce W^X. */
ENTRY(_start)

PHDRS
{
    text PT_LOAD FLAGS(5);   /* R+X */
    rodata PT_LOAD FLAGS(4); /* R */
    data PT_LOAD FLAGS(6);   /* R+W */
}

SECTIONS
{
    . = 0x0000000000600000;

    .text : ALIGN(4096) {
        *(.text .text.*)
    } :text

    . = ALIGN(4096);
    .rodata : {
        *(.rodata .rodata.*)
        *(.eh_frame .eh_frame.*)
    } :rodata

    . = ALIGN(4096);
    .data : {
        *(.data .data.*)
        *(.got .got.*)
    } :data

    . = ALIGN(4096);
    .bss : {
        *(COMMON)
        *(.bss .bss.*)
    } :data
}
