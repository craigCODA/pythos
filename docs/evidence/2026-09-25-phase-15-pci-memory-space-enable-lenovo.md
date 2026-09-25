# Phase 15 PCI Memory-Space Enable — Lenovo Acceptance

**Date:** 2026-09-25  
**Status:** Accepted physical evidence  
**ADR:** [0108](../decisions/0108-phase-15-pci-memory-space-enable-experiment.md)

## Hardware result

The corrected ISO completed the bounded Lenovo `81VS` experiment for the
Realtek controller at BDF `02:00:00`, vendor/device `10EC:C82F`.

The framebuffer evidence shows:

```text
orig     00100000
after    00100002
value    300034DB
restored 00100000
mse restored
no bus master
```

This proves that the profile temporarily set only PCI Memory Space Enable,
read the fixed Realtek register, restored the original low command word, and
did not enable PCI Bus Mastering. It does not claim controller initialization,
DMA, interrupts, packets, Wi-Fi association, or general physical networking.

## Photo evidence

- Source: `D:\Downloads\Mobile Devices\20260925_022359.jpg`
- Repository copy: [`2026-09-25-phase-15-pci-memory-space-enable-lenovo.jpg`](2026-09-25-phase-15-pci-memory-space-enable-lenovo.jpg)
- SHA-256: `662D76C1B61B83D1CAD096D71DD7C81176BBC6B10A9B4B39C1359F3131205319`
- Size: `2,037,559` bytes
- Tested ISO SHA-256: `4DC6855E8EE14036EF40D1EC88442839EABA58D4F79C6F64A7A6DBC989A33332`

## Acceptance conclusion

The physical Lenovo gate for ADR 0108 passed. The earlier violet-screen image
is retained as diagnostic history; it was not used as acceptance evidence.
