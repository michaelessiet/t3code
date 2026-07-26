import { FileCode2 } from "lucide-react";
import { useEffect } from "react";

import { ensurePierreIconSprite, resolvePierreIconForEntry } from "~/pierre-icons";

/**
 * Per-filetype icon from the file tree's Pierre sprite sheet, usable outside
 * the tree's shadow DOM (quick search, pickers). Falls back to a generic
 * code-file glyph when the path has no specific icon.
 */
export function PierreFileIcon({ path, className }: { path: string; className?: string }) {
  useEffect(() => {
    ensurePierreIconSprite();
  }, []);
  const icon = resolvePierreIconForEntry(path, "file");
  if (icon === null) {
    return <FileCode2 className={className} aria-hidden="true" />;
  }
  return (
    <svg
      className={className}
      viewBox="0 0 16 16"
      aria-hidden="true"
      data-icon-name={icon.name}
      data-icon-token={icon.token}
    >
      <use href={`#${icon.name.replace(/^#/, "")}`} />
    </svg>
  );
}
