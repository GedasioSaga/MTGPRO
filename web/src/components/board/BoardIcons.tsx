interface IconProps {
  className?: string
}

/** Ícones de zona. Traço fino, 24x24, herdam `currentColor`. */

export function HandIcon({ className }: IconProps) {
  return (
    <svg viewBox="0 0 24 24" fill="none" className={className} aria-hidden="true">
      <path
        d="M4 15.5 5.4 9.2a1.6 1.6 0 0 1 3.1.3l.3 2.3M8.8 11.8V6.1a1.6 1.6 0 0 1 3.2 0v5.4M12 11.5V7.2a1.6 1.6 0 0 1 3.2 0v4.6M15.2 12V9.4a1.5 1.5 0 0 1 3 0v5.9c0 3.1-2.2 5.2-5.3 5.2-2.6 0-4.1-.9-5.2-2.6L4 15.5"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  )
}

export function LibraryIcon({ className }: IconProps) {
  return (
    <svg viewBox="0 0 24 24" fill="none" className={className} aria-hidden="true">
      <rect x="5" y="4" width="14" height="16" rx="2" stroke="currentColor" strokeWidth="1.5" />
      <path d="M8.5 8h7M8.5 11.5h7M8.5 15h4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
    </svg>
  )
}

export function GraveyardIcon({ className }: IconProps) {
  return (
    <svg viewBox="0 0 24 24" fill="none" className={className} aria-hidden="true">
      <path
        d="M6 20V10a6 6 0 0 1 12 0v10"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
      />
      <path d="M4.5 20h15" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
      <path d="M12 8v6M9.5 10.5h5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
    </svg>
  )
}

export function ExileIcon({ className }: IconProps) {
  return (
    <svg viewBox="0 0 24 24" fill="none" className={className} aria-hidden="true">
      <circle cx="12" cy="12" r="7.5" stroke="currentColor" strokeWidth="1.5" />
      <path d="M12 7.5v9M7.5 12h9" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" transform="rotate(45 12 12)" />
    </svg>
  )
}

export function PoisonIcon({ className }: IconProps) {
  return (
    <svg viewBox="0 0 24 24" fill="none" className={className} aria-hidden="true">
      <path
        d="M10 3h4v4.2l3.4 8.2A3.6 3.6 0 0 1 14.1 21H9.9a3.6 3.6 0 0 1-3.3-5.6L10 7.2Z"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinejoin="round"
      />
      <path d="M7.4 14.5h9.2" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
    </svg>
  )
}

export function TapIcon({ className }: IconProps) {
  return (
    <svg viewBox="0 0 24 24" fill="none" className={className} aria-hidden="true">
      <path
        d="M6 5.5 17 15M17 15v-4.4M17 15h-4.4"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path d="M5 18.5h14" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
    </svg>
  )
}
