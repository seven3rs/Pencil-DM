/** How disk space is reserved before a download starts (aria2 `file-allocation`). */
export type FileAllocationMode = "falloc" | "none" | "prealloc";

/** The default when nothing is saved; mirrors `FileAllocation::default()` in crates/engine. */
export const DEFAULT_ALLOCATION: FileAllocationMode = "falloc";

export const ALLOCATION_MODES: { value: FileAllocationMode; label: string; hint: string }[] = [
  {
    value: "falloc",
    label: "Reserve instantly (recommended)",
    hint: "Claims the space right away without writing anything. Best on ext4, btrfs, xfs and NTFS.",
  },
  {
    value: "none",
    label: "Don't reserve",
    hint: "Skips reservation. Starts immediately on any drive, but the file can end up fragmented.",
  },
  {
    value: "prealloc",
    label: "Reserve by writing zeros (slow)",
    hint: "Works everywhere, but large files can take a long time before the download begins.",
  },
];

/** Narrows a saved setting to a known mode, falling back to the default. */
export function parseAllocation(value: string | undefined | null): FileAllocationMode {
  return ALLOCATION_MODES.some((m) => m.value === value) ? (value as FileAllocationMode) : DEFAULT_ALLOCATION;
}
