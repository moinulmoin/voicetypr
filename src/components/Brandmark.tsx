type BrandmarkProps = {
  className?: string;
  /** When provided, the mark is exposed to assistive tech with this label. */
  title?: string;
};

/**
 * The Voicetypr brandmark — the one-color "V" voice mark.
 *
 * Renders in `currentColor`, so set the color with a text utility on the element
 * itself or a parent (e.g. `text-sage`, `text-foreground`). Size it with width/height
 * utilities (e.g. `size-6`).
 */
export function Brandmark({ className, title }: BrandmarkProps) {
  return (
    <svg
      viewBox="100 96 312 320"
      className={className}
      fill="none"
      role={title ? "img" : undefined}
      aria-label={title}
      aria-hidden={title ? undefined : true}
    >
      <g stroke="currentColor" strokeLinecap="round" strokeLinejoin="round">
        <path d="M143 197 C166 288 201 350 240 393" strokeWidth="36" />
        <path d="M369 197 C346 288 311 350 272 393" strokeWidth="36" />
        <path d="M256 163 L256 338" strokeWidth="23" />
        <path d="M202 226 L202 257" strokeWidth="15" />
        <path d="M226 207 L226 276" strokeWidth="15" />
        <path d="M286 207 L286 276" strokeWidth="15" />
        <path d="M310 226 L310 257" strokeWidth="15" />
      </g>
      <circle cx="256" cy="124" r="18" fill="currentColor" />
    </svg>
  );
}
