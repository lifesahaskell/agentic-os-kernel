/* Flat ring-3 image: loaded at 0x400000 by kernel/src/user.rs::load_image.
   The first 40 bytes are the header the loader reads (abi::IMAGE_*). */
ENTRY(_start)

SECTIONS
{
  . = 0x400000;

  .aok_header : {
    QUAD(0x3130474D494B4F41) /* "AOKIMG01" little-endian */
    QUAD(_start)
    QUAD(_aok_handler)
    QUAD(__rx_end)
    QUAD(__mem_end)
  }

  .text : { *(.text .text.*) }
  .rodata : { *(.rodata .rodata.*) }

  . = ALIGN(4096);
  __rx_end = .;

  .data : { *(.data .data.*) }
  .bss : { *(.bss .bss.*) *(COMMON) }

  . = ALIGN(8);
  __mem_end = .;

  /DISCARD/ : {
    *(.eh_frame*)
    *(.comment)
    *(.note*)
    *(.debug*)
  }
}
