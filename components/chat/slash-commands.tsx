"use client";

import {
  BombIcon,
  ListIcon,
  PenLineIcon,
  PenSquareIcon,
  Trash2Icon,
  XIcon,
} from "lucide-react";
import { type ReactNode, useCallback, useEffect, useRef } from "react";
import { cn } from "@/lib/utils";

export type SlashCommand = {
  name: string;
  description: string;
  icon: ReactNode;
  action: string;
  shortcut?: string;
};

export const slashCommands: SlashCommand[] = [
  {
    action: "new",
    description: "Start a new chat",
    icon: <PenSquareIcon className="size-3.5" />,
    name: "new",
  },
  {
    action: "clear",
    description: "Clear current chat",
    icon: <Trash2Icon className="size-3.5" />,
    name: "clear",
  },
  {
    action: "rename",
    description: "Rename current chat",
    icon: <PenLineIcon className="size-3.5" />,
    name: "rename",
  },
  {
    action: "model",
    description: "Change the AI model",
    icon: <ListIcon className="size-3.5" />,
    name: "model",
  },
  {
    action: "delete",
    description: "Delete current chat",
    icon: <XIcon className="size-3.5" />,
    name: "delete",
  },
  {
    action: "purge",
    description: "Delete all chats",
    icon: <BombIcon className="size-3.5" />,
    name: "purge",
  },
];

type SlashCommandMenuProps = {
  query: string;
  onSelect: (command: SlashCommand) => void;
  onClose: () => void;
  selectedIndex: number;
};

function SlashCommandMenuItem({
  cmd,
  index,
  onSelect,
  selectedIndex,
}: {
  cmd: SlashCommand;
  index: number;
  onSelect: (command: SlashCommand) => void;
  selectedIndex: number;
}) {
  const handleClick = useCallback(() => {
    onSelect(cmd);
  }, [cmd, onSelect]);

  const handleMouseDown = useCallback(
    (e: React.MouseEvent<HTMLButtonElement>) => {
      e.preventDefault();
    },
    []
  );

  return (
    <button
      aria-selected={index === selectedIndex}
      className={cn(
        "flex w-full items-center gap-3 px-4 py-3 text-left text-sm transition-colors",
        index === selectedIndex ? "bg-accent" : "hover:bg-accent/70"
      )}
      data-selected={index === selectedIndex}
      onClick={handleClick}
      onMouseDown={handleMouseDown}
      role="option"
      type="button"
    >
      <div className="flex size-7 shrink-0 items-center justify-center text-muted-foreground">
        {cmd.icon}
      </div>
      <span className="font-mono text-sm text-foreground">/{cmd.name}</span>
      <span className="text-sm text-muted-foreground">{cmd.description}</span>
      {cmd.shortcut ? (
        <span className="ml-auto text-xs text-muted-foreground">
          {cmd.shortcut}
        </span>
      ) : null}
    </button>
  );
}

export function SlashCommandMenu({
  query,
  onSelect,
  onClose: _onClose,
  selectedIndex,
}: SlashCommandMenuProps) {
  const menuRef = useRef<HTMLDivElement>(null);
  const filtered = slashCommands.filter((cmd) =>
    cmd.name.startsWith(query.toLowerCase())
  );

  useEffect(() => {
    const selected = menuRef.current?.querySelector("[data-selected='true']");
    if (selected) {
      selected.scrollIntoView({ block: "nearest" });
    }
  }, []);

  if (filtered.length === 0) {
    return null;
  }

  return (
    <div
      aria-label="Slash commands"
      className="absolute right-0 bottom-full left-0 z-50 mb-2 overflow-hidden rounded-2xl border border-border bg-card shadow-[var(--shadow-float)]"
      id="slash-command-menu"
      ref={menuRef}
      role="listbox"
    >
      <div className="px-4 py-3 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
        Commands
      </div>
      <div className="max-h-64 overflow-y-auto pb-1 no-scrollbar">
        {filtered.map((cmd, index) => (
          <SlashCommandMenuItem
            cmd={cmd}
            index={index}
            key={cmd.name}
            onSelect={onSelect}
            selectedIndex={selectedIndex}
          />
        ))}
      </div>
    </div>
  );
}
