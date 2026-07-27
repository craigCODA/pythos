/* Explicit PHDRS (matching core/linker.ld's proven pattern): declaring every
 * program header ourselves means the linker emits *only* these, not the
 * default PT_GNU_STACK/PT_NOTE/etc. a plain SECTIONS-only script invites.
 * This keeps the ELF's program headers exactly PT_LOAD-only, matching what
 * core/src/user_elf.rs's loader validates. */
ENTRY(_start)

PHDRS
{
    text PT_LOAD FLAGS(5);   /* R+X */
    rodata PT_LOAD FLAGS(4); /* R */
    data PT_LOAD FLAGS(6);   /* R+W */
}

SECTIONS
{
    /* USER_VIRT_MIN in core/src/user_elf.rs is 0x0020_0000; this base
     * matches the existing adversarial-probe ELF fixtures. */
    . = 0x0000000000400000;

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
