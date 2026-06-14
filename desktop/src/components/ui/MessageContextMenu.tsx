import * as CM from "@radix-ui/react-context-menu";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { OVERLAY_ANIM } from "../../lib/anim";
import { api } from "../../api";
import { useStore } from "../../store";
import { displayName } from "../../lib/format";
import type { Message } from "../../types";

// Menu contextuel (clic droit) sur un message.
export function MessageContextMenu({
  message,
  channelId,
  mine,
  onReply,
  onEdit,
  children,
}: {
  message: Message;
  channelId: string;
  mine: boolean;
  onReply: (m: Message) => void;
  onEdit: (id: string) => void;
  children: ReactNode;
}) {
  const deleteMessage = useStore((s) => s.deleteMessage);
  const createThread = useStore((s) => s.createThread);
  const [confirmOpen, setConfirmOpen] = useState(false);
  // Shift maintenu au moment du clic « Supprimer » ⇒ on saute la confirmation (façon Discord).
  const shiftHeld = useRef(false);

  const copy = (text: string) => void navigator.clipboard?.writeText(text).catch(() => {});

  function onDelete() {
    if (shiftHeld.current) void deleteMessage(channelId, message.id);
    else setConfirmOpen(true);
  }

  return (
    <>
      <CM.Root>
        <CM.Trigger>{children}</CM.Trigger>
        <CM.Portal>
          <CM.Content className={`z-[60] min-w-[200px] rounded-xl bg-floating p-1.5 shadow-pop ring-1 ring-line ${OVERLAY_ANIM}`}>
            <Item onSelect={() => onReply(message)}>Répondre</Item>
            <Item onSelect={() => void createThread(channelId, message.content || "Nouveau fil")}>
              Créer un fil
            </Item>
            {message.content && <Item onSelect={() => copy(message.content)}>Copier le texte</Item>}
            {message.pinned ? (
              <Item onSelect={() => void api.unpinMessage(channelId, message.id).catch(() => {})}>
                Désépingler
              </Item>
            ) : (
              <Item onSelect={() => void api.pinMessage(channelId, message.id).catch(() => {})}>
                Épingler
              </Item>
            )}
            {mine && <Item onSelect={() => onEdit(message.id)}>Modifier</Item>}
            {mine && (
              <Item
                danger
                onSelect={(e) => {
                  // Radix transmet l'événement natif ; on y lit la touche Maj.
                  shiftHeld.current = !!(e as MouseEvent | KeyboardEvent)?.shiftKey;
                  onDelete();
                }}
              >
                Supprimer
              </Item>
            )}
            <CM.Separator className="my-1 h-px bg-white/10" />
            <Item onSelect={() => copy(message.id)}>Copier l'identifiant</Item>
          </CM.Content>
        </CM.Portal>
      </CM.Root>

      {confirmOpen && (
        <DeleteConfirm
          message={message}
          onCancel={() => setConfirmOpen(false)}
          onConfirm={() => {
            setConfirmOpen(false);
            void deleteMessage(channelId, message.id);
          }}
        />
      )}
    </>
  );
}

// Modale de confirmation de suppression : aperçu du message + Échap/Entrée.
function DeleteConfirm({
  message,
  onCancel,
  onConfirm,
}: {
  message: Message;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onCancel();
      if (e.key === "Enter") onConfirm();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onCancel, onConfirm]);

  return (
    <div
      className="fixed inset-0 z-[80] flex animate-overlay-in items-center justify-center bg-black/60 p-6"
      onClick={onCancel}
    >
      <div
        className="w-[440px] animate-pop-in overflow-hidden rounded-xl bg-modal shadow-2xl ring-1 ring-cardline"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="px-5 pb-3 pt-5">
          <h2 className="text-lg font-bold text-header">Supprimer le message</h2>
          <p className="mt-1 text-sm text-muted">
            Veux-tu vraiment supprimer ce message ? Cette action est irréversible.
          </p>
        </div>
        {/* Aperçu du message à supprimer. */}
        <div className="mx-5 mb-4 rounded-lg bg-deepest p-3 ring-1 ring-line">
          <div className="text-xs font-semibold text-header">{displayName(message.author)}</div>
          <div className="mt-0.5 line-clamp-3 whitespace-pre-wrap break-words text-sm text-normal">
            {message.content || (message.sticker ? "[autocollant]" : "[pièce jointe]")}
          </div>
        </div>
        <div className="flex justify-end gap-2 bg-deepest/40 px-5 py-3">
          <button
            onClick={onCancel}
            className="rounded-lg px-4 py-2 text-sm font-medium text-normal hover:underline"
          >
            Annuler
          </button>
          <button
            onClick={onConfirm}
            autoFocus
            className="rounded-lg bg-dnd px-4 py-2 text-sm font-semibold text-white hover:opacity-90"
          >
            Supprimer
          </button>
        </div>
        <p className="pb-3 text-center text-[11px] text-muted">
          Astuce : maintiens <kbd className="rounded bg-black/30 px-1">Maj</kbd> en cliquant
          « Supprimer » pour sauter cette confirmation.
        </p>
      </div>
    </div>
  );
}

function Item({
  children,
  onSelect,
  danger,
}: {
  children: ReactNode;
  onSelect: (event: Event) => void;
  danger?: boolean;
}) {
  return (
    <CM.Item
      onSelect={onSelect}
      className={`cursor-pointer rounded px-2 py-1.5 text-sm outline-none transition-colors duration-150 data-[highlighted]:translate-x-0.5 ${
        danger
          ? "text-dnd data-[highlighted]:bg-dnd data-[highlighted]:text-white"
          : "text-normal data-[highlighted]:bg-accent data-[highlighted]:text-white"
      }`}
    >
      {children}
    </CM.Item>
  );
}
