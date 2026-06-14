import { BarChart3, Check } from "lucide-react";
import { useStore } from "../store";
import type { Poll } from "../types";

// Rendu d'un sondage attaché à un message (barres de résultats + vote).
export function PollView({ poll, channelId }: { poll: Poll; channelId: string }) {
  const castVote = useStore((s) => s.castVote);
  const total = poll.answers.reduce((n, a) => n + a.vote_count, 0);
  const closed = poll.finished;

  function vote(answerId: number, meVoted: boolean) {
    if (closed) return;
    if (poll.multiselect) {
      const current = poll.answers.filter((a) => a.me_voted).map((a) => a.answer_id);
      const next = meVoted ? current.filter((id) => id !== answerId) : [...current, answerId];
      void castVote(channelId, poll.message_id, next);
    } else {
      void castVote(channelId, poll.message_id, meVoted ? [] : [answerId]);
    }
  }

  return (
    <div className="mt-1 max-w-md rounded-lg border border-line bg-sidebar p-3">
      <div className="mb-1 flex items-center gap-1.5 text-xs font-semibold uppercase text-muted">
        <BarChart3 size={14} />
        Sondage{poll.multiselect ? " · choix multiple" : ""}
      </div>
      <div className="mb-2 font-semibold text-header">{poll.question}</div>
      <div className="space-y-1.5">
        {poll.answers.map((a) => {
          const pct = total > 0 ? Math.round((a.vote_count / total) * 100) : 0;
          return (
            <button
              key={a.answer_id}
              onClick={() => vote(a.answer_id, a.me_voted)}
              disabled={closed}
              className={`relative w-full overflow-hidden rounded border px-3 py-2 text-left ${
                a.me_voted ? "border-blurple" : "border-transparent"
              } ${closed ? "cursor-default" : "hover:border-white/30"}`}
            >
              {/* Barre de résultat. */}
              <span
                className="absolute inset-y-0 left-0 bg-blurple/20"
                style={{ width: `${pct}%` }}
                aria-hidden
              />
              <span className="relative flex items-center justify-between">
                <span className="flex items-center gap-1.5 text-sm text-normal">
                  {a.me_voted && <Check size={14} className="text-blurple" />}
                  {a.text}
                </span>
                <span className="text-xs text-muted">{pct}%</span>
              </span>
            </button>
          );
        })}
      </div>
      <div className="mt-2 text-xs text-muted">
        {total} vote{total > 1 ? "s" : ""}
        {closed ? " · terminé" : ""}
      </div>
    </div>
  );
}
