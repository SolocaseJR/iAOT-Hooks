# Mach-O Patcher

The planned offline writer appends `__IAOTCFG` as read/write fixed-layout data and `__IAOTCAVE` as read/execute code generated from audited templates. A cave can load a flag, take the normal path when disabled, invoke a precompiled callback or small generic action when enabled, replay a safe displaced instruction, and branch back.

Audited actions are `ret_if_flag`, `zero_register_if_flag`, `callback_if_flag`, `callback_return_zero_if_flag`, `callback_after`, `branch_to_callback`, and `branch_to_replacement`. There is no arbitrary native-code string input.

Safety requires exact-byte checks, aligned/range-checked branches, unambiguous patterns/xrefs, non-overlapping VM ranges, config layout checks, stale signature removal, reopen/parse verification, redirect decoding where disassembly is available, and refusal to relocate PC-relative displaced instructions without a proven relocation strategy.

Implemented behavior includes thin ARM64 `MH_EXECUTE` parsing, direct/pattern/xref/symbol locators, exact-byte checks, `__IAOTCFG` and `__IAOTCAVE` insertion, audited cave emission, stale `LC_CODE_SIGNATURE` removal and blob clearing, ARM64 redirects, and reopen verification. Observational callbacks preserve caller-saved general registers, q0–q7, LR, and NZCV.

The writer intentionally refuses fat Mach-O files, PC-relative displaced instructions, branches outside ARM64 range, ambiguous matches without indices, overlapping VM ranges, and inputs lacking enough padding between load commands and the first section. It does not shift existing sections to manufacture header space. `branch_to_replacement` requires a resolvable symbol; callback actions use precompiled callback pointers placed in `RuntimeConfig` by signed runtime code.
