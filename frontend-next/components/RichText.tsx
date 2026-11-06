"use client";

/**
 * A very small text renderer.
 *
 * The interface does not accept arbitrary HTML - messages are plain text
 * with a handful of marks. Rendering them here keeps the message body safe
 * by construction: nothing a student types can become markup.
 */

import { Fragment, type ReactNode } from "react";

/** Render a message body with bold, italic, code, mentions and links. */
export function RichText({ text }: { text: string }) {
  return <>{render(text)}</>;
}

/** The tokenizer. */
function render(input: string): ReactNode {
  const nodes: ReactNode[] = [];
  let key = 0;

  // `code`, **bold**, _italic_, @mention and http links, in that order.
  const pattern =
    /(`[^`\n]+`)|(\*\*[^*\n]+\*\*)|(_[^_\n]+_)|(@[a-z0-9._-]{2,32})|(https?:\/\/[^\s]+)/gi;

  let lastIndex = 0;
  let match: RegExpExecArray | null;

  while ((match = pattern.exec(input)) !== null) {
    if (match.index > lastIndex) {
      nodes.push(
        <Fragment key={key++}>{input.slice(lastIndex, match.index)}</Fragment>,
      );
    }
    const token = match[0];

    if (token.startsWith("`")) {
      nodes.push(
        <code key={key++} className="msg-code">
          {token.slice(1, -1)}
        </code>,
      );
    } else if (token.startsWith("**")) {
      nodes.push(
        <strong key={key++} className="font-semibold text-fg">
          {token.slice(2, -2)}
        </strong>,
      );
    } else if (token.startsWith("_")) {
      nodes.push(
        <em key={key++} className="italic">
          {token.slice(1, -1)}
        </em>,
      );
    } else if (token.startsWith("@")) {
      nodes.push(
        <span
          key={key++}
          className="rounded bg-accent-soft px-1 font-medium text-accent"
        >
          {token}
        </span>,
      );
    } else {
      nodes.push(
        <a
          key={key++}
          href={token}
          target="_blank"
          rel="noopener noreferrer nofollow"
          className="text-accent underline underline-offset-2 hover:brightness-110"
        >
          {token}
        </a>,
      );
    }

    lastIndex = match.index + token.length;
  }

  if (lastIndex < input.length) {
    nodes.push(<Fragment key={key++}>{input.slice(lastIndex)}</Fragment>);
  }

  return nodes;
}
