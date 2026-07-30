# eMMC Identification Probe Implementation Plan

Date: 2026-07-30

## Goal

Extend the hardware probe so a laptop with an eMMC device can prove that the
SDHCI controller reaches the card-identification phase without any block data
path access.

## Steps

1. Add SDHCI command-path helpers.
   - Add command register encoding for no-response, short-response, and
     136-bit-response commands.
   - Enable command-complete and error status reporting without enabling SDHCI
     interrupt signals.
   - Add bounded command-inhibit and command-complete polling.
   - Add status clearing through normal and error interrupt status registers.

2. Add eMMC identification logic.
   - Set a conservative identification clock from SDHCI capabilities.
   - Run `CMD0`, `CMD1`, `CMD2`, `CMD3`, and `CMD9`.
   - Return a typed identification report or typed failure.
   - Keep transfer mode, block size/count, DMA/ADMA, and buffer data registers
     untouched.

3. Wire boot and screen reporting.
   - Call identification only after SDHCI initialization succeeds.
   - Emit OCR, RCA, CID, CSD, and completion/error serial markers.
   - Render a compact `emmc id` screen while preserving `no disk writes`.

4. Add QEMU eMMC acceptance.
   - Extend the QEMU runner with an opt-in `--emmc` device mode layered on
     `--sdhci`.
   - Extend the hardware-probe acceptance test to require the eMMC
     identification marker in that mode.

5. Verify and ship.
   - Run focused Rust unit tests.
   - Run the QEMU hardware-probe acceptance test.
   - Build the boot artifacts.
   - Commit, push, and deploy the updated ESP to `F:`.
