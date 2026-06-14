import { useState } from "react";
import { Plus, X } from "lucide-react";
import { api } from "../api";
import { Modal } from "./ServerRail";
import { Spinner } from "./ui/Spinner";

const DURATIONS: { label: string; hours: number }[] = [
  { label: "1 heure", hours: 1 },
  { label: "24 heures", hours: 24 },
  { label: "3 jours", hours: 72 },
  { label: "7 jours", hours: 168 },
  { label: "Sans expiration", hours: 0 },
];

export function CreatePollModal({ channelId, onClose }: { channelId: string; onClose: () => void }) {
  const [question, setQuestion] = useState("");
  const [answers, setAnswers] = useState<string[]>(["", ""]);
  const [multiselect, setMultiselect] = useState(false);
  const [duration, setDuration] = useState(24);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Un sondage exige au moins deux réponses (un seul choix n'a pas de sens).
  const valid = question.trim() && answers.filter((a) => a.trim()).length >= 2;

  async function submit() {
    setBusy(true);
    setError(null);
    try {
      await api.createPoll(channelId, {
        question: question.trim(),
        answers: answers.map((a) => a.trim()).filter(Boolean),
        multiselect,
        duration_hours: duration,
      });
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Échec.");
      setBusy(false);
    }
  }

  return (
    <Modal onClose={onClose}>
      <div className="w-[460px] rounded-xl bg-modal p-6 ring-1 ring-cardline surface-card">
        <h2 className="mb-4 text-xl font-bold text-header">Créer un sondage</h2>

        <label className="mb-1.5 block text-xs font-bold uppercase tracking-wide text-subtext">
          Question
        </label>
        <input
          autoFocus
          value={question}
          onChange={(e) => setQuestion(e.target.value)}
          maxLength={300}
          placeholder="Quelle est ta question ?"
          className="mb-4 w-full rounded-lg bg-deepest px-3 py-2.5 text-normal outline-none ring-1 ring-transparent focus:ring-accent"
        />

        <label className="mb-1.5 block text-xs font-bold uppercase tracking-wide text-subtext">
          Réponses
        </label>
        <div className="mb-2 space-y-2">
          {answers.map((a, i) => (
            <div key={i} className="flex gap-2">
              <input
                value={a}
                maxLength={55}
                onChange={(e) =>
                  setAnswers((arr) => arr.map((x, j) => (j === i ? e.target.value : x)))
                }
                placeholder={`Réponse ${i + 1}`}
                className="flex-1 rounded-lg bg-deepest px-3 py-2 text-normal outline-none ring-1 ring-transparent focus:ring-accent"
              />
              {answers.length > 1 && (
                <button
                  onClick={() => setAnswers((arr) => arr.filter((_, j) => j !== i))}
                  className="rounded px-2 text-muted hover:text-dnd"
                  title="Retirer"
                >
                  <X size={16} />
                </button>
              )}
            </div>
          ))}
        </div>
        {answers.length < 10 && (
          <button
            onClick={() => setAnswers((arr) => [...arr, ""])}
            className="mb-4 flex items-center gap-1.5 text-sm text-link hover:underline"
          >
            <Plus size={14} />
            Ajouter une réponse
          </button>
        )}

        <div className="mb-4 flex items-center justify-between gap-3">
          <label className="flex cursor-pointer items-center gap-2 text-sm text-normal">
            <input
              type="checkbox"
              checked={multiselect}
              onChange={(e) => setMultiselect(e.target.checked)}
              className="h-4 w-4 accent-[#5865f2]"
            />
            Choix multiple
          </label>
          <select
            value={duration}
            onChange={(e) => setDuration(Number(e.target.value))}
            className="rounded bg-deepest px-3 py-2 text-sm text-normal outline-none"
          >
            {DURATIONS.map((d) => (
              <option key={d.hours} value={d.hours}>
                {d.label}
              </option>
            ))}
          </select>
        </div>

        {error && <p className="mb-3 text-sm text-dnd">{error}</p>}

        <div className="flex justify-end gap-3">
          <button onClick={onClose} className="px-4 py-2 text-sm text-normal hover:underline">
            Annuler
          </button>
          <button
            onClick={() => void submit()}
            disabled={busy || !valid}
            className="pressable inline-flex items-center justify-center gap-2 rounded-lg btn-accent px-5 py-2 text-sm font-medium text-white disabled:cursor-not-allowed disabled:opacity-60"
          >
            {busy && <Spinner size={14} />}Créer
          </button>
        </div>
      </div>
    </Modal>
  );
}
