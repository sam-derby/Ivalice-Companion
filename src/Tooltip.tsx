import { useEffect, useId, useRef, useState, type ReactNode } from 'react';
import { createPortal } from 'react-dom';

/** A small info icon carrying a description, for text without its own icon. */
export function InfoHint({
  text,
  label,
}: {
  text: string | undefined;
  label: string;
}) {
  if (!text) return null;
  return (
    <Tooltip text={text}>
      <span
        className="workspace-info"
        role="img"
        aria-label={`About ${label}`}
      />
    </Tooltip>
  );
}

export function Tooltip({
  text,
  children,
  focusable = true,
}: {
  text: string | undefined;
  children: ReactNode;
  focusable?: boolean;
}) {
  const id = useId();
  const hideTimer = useRef<number | undefined>(undefined);
  function keepOpen() {
    window.clearTimeout(hideTimer.current);
  }
  function hideSoon() {
    keepOpen();
    hideTimer.current = window.setTimeout(() => {
      setPosition(null);
    }, 200);
  }
  useEffect(
    () => () => {
      window.clearTimeout(hideTimer.current);
    },
    [],
  );
  const [position, setPosition] = useState<{
    left: number;
    top: number;
  } | null>(null);
  function show(anchor: HTMLElement) {
    keepOpen();
    if (!text) return;
    const rect = anchor.getBoundingClientRect();
    const rem =
      Number.parseFloat(getComputedStyle(document.documentElement).fontSize) ||
      16;
    const width = Math.min(24 * rem, window.innerWidth - 16);
    const height = Math.min(12 * rem, window.innerHeight - 16);
    setPosition({
      left: Math.max(8, Math.min(rect.left, window.innerWidth - width - 8)),
      top: Math.max(
        8,
        Math.min(rect.bottom + 6, window.innerHeight - height - 8),
      ),
    });
  }
  return (
    <span
      className="workspace-hint"
      tabIndex={text && focusable ? 0 : undefined}
      aria-describedby={position ? id : undefined}
      onMouseEnter={(event) => {
        show(event.currentTarget);
      }}
      onMouseLeave={(event) => {
        if (!event.currentTarget.contains(document.activeElement)) hideSoon();
      }}
      onFocus={(event) => {
        show(event.currentTarget);
      }}
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget)) {
          keepOpen();
          setPosition(null);
        }
      }}
      onKeyDown={(event) => {
        if (event.key === 'Escape') {
          keepOpen();
          setPosition(null);
        }
      }}
    >
      {children}
      {position &&
        text &&
        createPortal(
          <span
            id={id}
            role="tooltip"
            className="workspace-tooltip"
            style={position}
            onMouseEnter={keepOpen}
            onMouseLeave={hideSoon}
          >
            {text.replace(/<\/?color(?:=\d+)?>/g, '')}
          </span>,
          document.body,
        )}
    </span>
  );
}
