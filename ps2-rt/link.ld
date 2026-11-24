/*
 * Linker script for bare-metal PS2 (Emotion Engine) executables.
 *
 * The PS2 BIOS / ELF loaders (PCSX2's "boot ELF", OPL, ps2link, ...) load
 * the executable's segments directly into EE main RAM and jump to the ELF
 * entry point in kernel mode, with the MMU already set up so that low
 * addresses map directly to physical RAM. 0x00100000 (1 MiB) is the
 * conventional load address used by PS2 homebrew, leaving the first MiB
 * free for BIOS/kernel use.
 */

ENTRY(_start)

MEMORY
{
  /* EE main RAM is 32 MiB; reserve the first 1 MiB. */
  ram (rwx) : ORIGIN = 0x00100000, LENGTH = 32M - 1M
}

SECTIONS
{
  . = ORIGIN(ram);

  .text : ALIGN(4) {
    *(.text.start)
    *(.text*)
  } > ram

  .rodata : ALIGN(4) {
    *(.rodata*)
  } > ram

  .data : ALIGN(8) {
    *(.data*)
  } > ram

  /*
   * _gp is set up on boot (see boot.rs) defensively: with "+noabicalls"
   * and a static relocation model, generated code should mostly use
   * absolute addressing rather than $gp-relative small-data access, but
   * this costs nothing to provide if the compiler emits it anyway.
   */
  _gp = . + 0x8000;

  .sdata : ALIGN(8) {
    *(.sdata*)
  } > ram

  .got : ALIGN(8) {
    *(.got*)
  } > ram

  .bss (NOLOAD) : ALIGN(8) {
    __bss_start = .;
    *(.sbss*)
    *(.bss*)
    *(COMMON)
    __bss_end = .;
  } > ram

  . = ALIGN(16);
  _heap_start = .;

  _stack_top = ORIGIN(ram) + LENGTH(ram);

  /DISCARD/ : {
    *(.comment)
    *(.pdr)
    *(.reginfo)
    *(.MIPS.abiflags)
    *(.note*)
  }
}
