import { useEffect, useRef, useState } from "react";
import { MessageSquare, Upload, X } from "lucide-react";
import { api } from "../api";
import { useStore } from "../store";
import type { Channel, Message } from "../types";
import { MessageList } from "./MessageList";
import { Composer, type StagedUpload } from "./ChatView";
import { ChatSkeleton } from "./ui/Skeleton";

const MAX_ATTACHMENT_BYTES = 1024 * 1024 * 1024;

// Discussion textuelle intégrée d'un salon vocal (panneau de droite, façon Discord).
// Le backend n'impose aucun type de salon pour les messages → on réutilise toute la pile texte.
export function VoiceTextChat({ channel, guildId }: { channel: Channel; guildId: string }) {
  const messages = useStore((s) => s.messagesByChannel[channel.id]);
  const loadMessages = useStore((s) => s.loadMessages);
  const markRead = useStore((s) => s.markRead);
  const setError = useStore((s) => s.setError);
  const setVoiceTextOpen = useStore((s) => s.setVoiceTextOpen);

  const [replyTarget, setReplyTarget] = useState<Message | null>(null);
  const [uploads, setUploads] = useState<StagedUpload[]>([]);
  const [dragging, setDragging] = useState(false);
  const dragDepth = useRef(0);

  // Charge les messages et marque comme lu à l'ouverture / changement de salon.
  useEffect(() => {
    void loadMessages(channel.id);
    markRead(channel.id);
    setReplyTarget(null);
    setUploads((u) => {
      u.forEach((x) => x.status === "uploading" && x.abort());
      return [];
    });
  }, [channel.id, loadMessages, markRead]);

  // Uploads stagés annulables (même modèle que ChatView).
  function onFiles(files: FileList | File[] | null) {
    if (!files || files.length === 0) return;
    for (const f of Array.from(files)) {
      if (f.size > MAX_ATTACHMENT_BYTES) {
        setError(`« ${f.name} » dépasse la limite de 1 Go.`);
        continue;
      }
      const localId = `${f.name}-${f.size}-${performance.now()}-${Math.random()}`;
      const controller = new AbortController();
      setUploads((u) => [
        ...u,
        { localId, name: f.name, size: f.size, type: f.type, status: "uploading", abort: () => controller.abort() },
      ]);
      api
        .uploadAttachmentAbortable(channel.id, f, controller.signal)
        .then((att) =>
          setUploads((u) =>
            u.map((x) => (x.localId === localId ? { ...x, status: "done", attachment: att } : x)),
          ),
        )
        .catch((e) => {
          if (controller.signal.aborted) {
            setUploads((u) => u.filter((x) => x.localId !== localId));
            return;
          }
          setUploads((u) => u.map((x) => (x.localId === localId ? { ...x, status: "error" } : x)));
          setError(e instanceof Error ? e.message : "Échec du téléversement.");
        });
    }
  }

  function cancelUpload(localId: string) {
    setUploads((u) => {
      u.find((x) => x.localId === localId)?.abort();
      return u.filter((x) => x.localId !== localId);
    });
  }

  return (
    <aside
      className="relative flex w-[400px] shrink-0 flex-col border-l border-line bg-chat"
      onDragEnter={(e) => {
        e.preventDefault();
        dragDepth.current += 1;
        if (e.dataTransfer.types.includes("Files")) setDragging(true);
      }}
      onDragOver={(e) => e.preventDefault()}
      onDragLeave={() => {
        dragDepth.current -= 1;
        if (dragDepth.current <= 0) setDragging(false);
      }}
      onDrop={(e) => {
        e.preventDefault();
        dragDepth.current = 0;
        setDragging(false);
        void onFiles(e.dataTransfer.files);
      }}
    >
      <header className="flex h-12 shrink-0 items-center gap-2 border-b border-line px-4 shadow-sm">
        <MessageSquare size={18} className="text-muted" />
        <h2 className="min-w-0 flex-1 truncate font-semibold text-header">{channel.name}</h2>
        <button
          onClick={() => setVoiceTextOpen(false)}
          title="Fermer la discussion"
          className="pressable rounded p-1.5 text-interactive-normal outline-none hover:bg-hover hover:text-interactive-hover"
        >
          <X size={18} />
        </button>
      </header>

      {messages === undefined ? (
        <ChatSkeleton />
      ) : (
        <MessageList
          key={channel.id}
          messages={messages}
          channelId={channel.id}
          guildId={guildId}
          onReply={setReplyTarget}
          channelName={channel.name}
        />
      )}

      <Composer
        key={`vc-composer-${channel.id}`}
        channelId={channel.id}
        title={channel.name}
        guildId={guildId}
        replyTarget={replyTarget}
        onClearReply={() => setReplyTarget(null)}
        uploads={uploads}
        setUploads={setUploads}
        onCancelUpload={cancelUpload}
        onFiles={onFiles}
      />

      {dragging && (
        <div className="animate-overlay-in pointer-events-none absolute inset-2 z-40 flex flex-col items-center justify-center rounded-xl border-2 border-dashed border-accent bg-accent/10">
          <Upload size={40} className="text-accent" />
          <p className="mt-2 text-sm font-semibold text-header">Dépose pour téléverser</p>
        </div>
      )}
    </aside>
  );
}
