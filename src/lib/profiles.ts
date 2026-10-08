import type { PartChoice, Profile } from "./types";

export const PARTS: { key: keyof PartChoice; label: string }[] = [
  { key: "apps", label: "Apps" },
  { key: "processing", label: "Mic & processing" },
  { key: "headphones", label: "Headphones" },
  { key: "defaultOutput", label: "Default output" },
];

/** Labels of the parts a profile controls. */
export function partLabels(p: Profile): string[] {
  return PARTS.filter((part) => p.parts[part.key] !== null).map((part) => part.label);
}
