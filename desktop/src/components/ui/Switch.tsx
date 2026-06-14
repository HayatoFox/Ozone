// Interrupteur premium réutilisable — piste + pastille animées (ressort + ombre).
// Remplace les toggles maison dupliqués (ServerSettings, OverviewPage, AutomodPage).
export function Switch({
  on,
  onToggle,
  disabled,
  title,
  size = "md",
}: {
  on: boolean;
  onToggle: () => void;
  disabled?: boolean;
  title?: string;
  size?: "sm" | "md";
}) {
  const dims =
    size === "sm"
      ? { track: "h-5 w-9", knob: "h-3.5 w-3.5", on: "translate-x-[18px]" }
      : { track: "h-6 w-11", knob: "h-4 w-4", on: "translate-x-5" };
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      title={title}
      disabled={disabled}
      onClick={onToggle}
      className={`pressable relative flex ${dims.track} shrink-0 items-center rounded-full px-0.5 transition-colors duration-200 disabled:opacity-50 ${
        on ? "bg-online" : "bg-white/15"
      }`}
    >
      <span
        className={`${dims.knob} rounded-full bg-white shadow-sm transition-transform duration-200 ease-[cubic-bezier(0.34,1.56,0.64,1)] ${
          on ? dims.on : "translate-x-0.5"
        }`}
      />
    </button>
  );
}
