import type { SVGProps } from "react";

export type IconName =
  | "brand"
  | "library"
  | "online"
  | "inPerson"
  | "upload"
  | "settings"
  | "lock"
  | "search"
  | "filter"
  | "sort"
  | "open"
  | "rename"
  | "delete"
  | "media"
  | "comparison"
  | "minutes"
  | "pause"
  | "resume"
  | "analyze"
  | "stop"
  | "copy"
  | "close"
  | "shield"
  | "eye"
  | "eyeOff";

interface Props extends Omit<SVGProps<SVGSVGElement>, "name"> {
  name: IconName;
  size?: number;
}

export function Icon({ name, size = 18, ...props }: Props) {
  const shared = {
    width: size,
    height: size,
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.8,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    "aria-hidden": true,
    focusable: false,
    ...props,
  };

  if (name === "brand") {
    return (
      <svg {...shared}>
        <path d="M4 9v6" />
        <path d="M8 6v12" />
        <path d="M12 3v18" />
        <path d="M16 7v10" />
        <path d="M20 10v4" />
      </svg>
    );
  }

  if (name === "library") {
    return (
      <svg {...shared}>
        <rect x="4" y="4" width="16" height="16" rx="2" />
        <path d="M8 4v16" />
        <path d="M12 8h5" />
        <path d="M12 12h5" />
      </svg>
    );
  }

  if (name === "online") {
    return (
      <svg {...shared}>
        <rect x="3" y="5" width="13" height="14" rx="2" />
        <path d="m16 10 5-3v10l-5-3" />
      </svg>
    );
  }

  if (name === "inPerson") {
    return (
      <svg {...shared}>
        <circle cx="8" cy="8" r="3" />
        <circle cx="16" cy="8" r="3" />
        <path d="M3 20c.6-4 2.5-6 5-6s4.4 2 5 6" />
        <path d="M11 20c.6-4 2.5-6 5-6s4.4 2 5 6" />
      </svg>
    );
  }

  if (name === "upload") {
    return (
      <svg {...shared}>
        <path d="M12 16V4" />
        <path d="m7 9 5-5 5 5" />
        <path d="M5 14v5h14v-5" />
      </svg>
    );
  }

  if (name === "settings") {
    return (
      <svg {...shared}>
        <circle cx="12" cy="12" r="3" />
        <path d="M19.4 15a1.8 1.8 0 0 0 .36 2l.07.07-2.12 2.12-.07-.07a1.8 1.8 0 0 0-2-.36 1.8 1.8 0 0 0-1.08 1.65V20.5h-3v-.1A1.8 1.8 0 0 0 10.5 18.8a1.8 1.8 0 0 0-2 .36l-.07.07-2.12-2.12.07-.07a1.8 1.8 0 0 0 .36-2A1.8 1.8 0 0 0 5.1 14H5v-3h.1a1.8 1.8 0 0 0 1.65-1.08 1.8 1.8 0 0 0-.36-2l-.07-.07 2.12-2.12.07.07a1.8 1.8 0 0 0 2 .36A1.8 1.8 0 0 0 11.6 4.5h.1v-3h3v.1a1.8 1.8 0 0 0 1.08 1.65 1.8 1.8 0 0 0 2-.36l.07-.07 2.12 2.12-.07.07a1.8 1.8 0 0 0-.36 2A1.8 1.8 0 0 0 21.2 8H21.3v3h-.1A1.8 1.8 0 0 0 19.55 12.1 1.8 1.8 0 0 0 19.4 15Z" transform="scale(.8) translate(3 3)" />
      </svg>
    );
  }

  if (name === "lock" || name === "shield") {
    return (
      <svg {...shared}>
        {name === "shield" ? <path d="M12 3 5 6v5c0 4.6 2.7 8.2 7 10 4.3-1.8 7-5.4 7-10V6l-7-3Z" /> : null}
        <rect x="7" y="10" width="10" height="9" rx="2" />
        <path d="M9 10V7a3 3 0 0 1 6 0v3" />
      </svg>
    );
  }


  if (name === "eye" || name === "eyeOff") {
    return (
      <svg {...shared}>
        <path d="M2.5 12s3.5-6 9.5-6 9.5 6 9.5 6-3.5 6-9.5 6-9.5-6-9.5-6Z" />
        {name === "eye" ? <circle cx="12" cy="12" r="2.8" /> : null}
        {name === "eyeOff" ? (
          <>
            <path d="M9.8 9.8a3 3 0 0 0 4.4 4.4" />
            <path d="M4 4l16 16" />
          </>
        ) : null}
      </svg>
    );
  }

  if (name === "search") {
    return (
      <svg {...shared}>
        <circle cx="11" cy="11" r="7" />
        <path d="m20 20-4-4" />
      </svg>
    );
  }

  if (name === "filter") {
    return (
      <svg {...shared}>
        <path d="M4 5h16" />
        <path d="M7 12h10" />
        <path d="M10 19h4" />
      </svg>
    );
  }

  if (name === "sort") {
    return (
      <svg {...shared}>
        <path d="M8 6h10" />
        <path d="M8 12h7" />
        <path d="M8 18h4" />
        <path d="M4 4v16" />
        <path d="m2 18 2 2 2-2" />
      </svg>
    );
  }

  if (name === "open") {
    return (
      <svg {...shared}>
        <path d="M14 5h5v5" />
        <path d="m10 14 9-9" />
        <path d="M19 13v6H5V5h6" />
      </svg>
    );
  }

  if (name === "rename") {
    return (
      <svg {...shared}>
        <path d="m4 20 4.5-1 10-10a2.1 2.1 0 0 0-3-3l-10 10L4 20Z" />
        <path d="m13.5 7.5 3 3" />
      </svg>
    );
  }

  if (name === "delete") {
    return (
      <svg {...shared}>
        <path d="M4 7h16" />
        <path d="M9 7V4h6v3" />
        <path d="m7 7 1 13h8l1-13" />
        <path d="M10 11v5" />
        <path d="M14 11v5" />
      </svg>
    );
  }

  if (name === "media") {
    return (
      <svg {...shared}>
        <rect x="3" y="5" width="18" height="14" rx="2" />
        <path d="m8 14 2.5-3 2 2 2.5-3 3 4" />
        <circle cx="8" cy="9" r="1" />
      </svg>
    );
  }

  if (name === "comparison") {
    return (
      <svg {...shared}>
        <path d="M8 4H4v16h4" />
        <path d="M16 4h4v16h-4" />
        <path d="M10 8h4" />
        <path d="m12 6 2 2-2 2" />
        <path d="M14 16h-4" />
        <path d="m12 14-2 2 2 2" />
      </svg>
    );
  }

  if (name === "minutes") {
    return (
      <svg {...shared}>
        <path d="M6 3h9l3 3v15H6z" />
        <path d="M15 3v4h4" />
        <path d="M9 11h6" />
        <path d="M9 15h6" />
      </svg>
    );
  }

  if (name === "pause") {
    return (
      <svg {...shared}>
        <path d="M9 5v14" />
        <path d="M15 5v14" />
      </svg>
    );
  }

  if (name === "resume") {
    return (
      <svg {...shared}>
        <path d="m8 5 11 7-11 7z" />
      </svg>
    );
  }

  if (name === "analyze") {
    return (
      <svg {...shared}>
        <path d="m12 3 1.2 4.8L18 9l-4.8 1.2L12 15l-1.2-4.8L6 9l4.8-1.2L12 3Z" />
        <path d="m18 15 .6 2.4L21 18l-2.4.6L18 21l-.6-2.4L15 18l2.4-.6L18 15Z" />
      </svg>
    );
  }

  if (name === "stop") {
    return (
      <svg {...shared}>
        <circle cx="12" cy="12" r="9" />
        <rect x="9" y="9" width="6" height="6" rx="1" />
      </svg>
    );
  }

  if (name === "copy") {
    return (
      <svg {...shared}>
        <rect x="8" y="8" width="11" height="11" rx="2" />
        <path d="M16 8V5H5v11h3" />
      </svg>
    );
  }

  return (
    <svg {...shared}>
      <path d="M6 6l12 12" />
      <path d="M18 6 6 18" />
    </svg>
  );
}
