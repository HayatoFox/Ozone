import { memberTopColorRole, roleNameStyle, type RoleNameStyle, useStore } from "../store";
import type { Member, Role } from "../types";
import { Avatar } from "./Avatar";
import { UserPopover } from "./ProfilePopout";
import { ListSkeleton } from "./ui/Skeleton";
import { displayName } from "../lib/format";

const ONLINE = new Set(["online", "idle", "dnd"]);

export function MemberList({ guildId }: { guildId: string }) {
  const rawMembers = useStore((s) => s.membersByGuild[guildId]);
  const members = rawMembers ?? [];
  const roles = useStore((s) => s.rolesByGuild[guildId]) ?? [];
  const presences = useStore((s) => s.presences);

  if (rawMembers === undefined) {
    return (
      <div className="w-60 shrink-0 bg-sidebar py-4">
        <ListSkeleton rows={8} />
      </div>
    );
  }

  const online = members.filter((m) => ONLINE.has(presences[m.user.id] ?? "offline"));
  const offline = members.filter((m) => !ONLINE.has(presences[m.user.id] ?? "offline"));

  // Rôles « affichés séparément » (hoist), du plus haut au plus bas.
  const hoisted = roles.filter((r) => r.hoist).sort((a, b) => b.position - a.position);
  const topHoist = (m: Member): Role | undefined =>
    hoisted.find((r) => m.roles.includes(r.id));

  // Groupes : un par rôle hoisté (membres en ligne), puis « En ligne » (sans rôle hoisté).
  // L'en-tête de groupe n'est PAS coloré (style Discord) : seule la couleur du pseudo change.
  const groups: { key: string; title: string; members: Member[] }[] = [];
  for (const role of hoisted) {
    const list = online.filter((m) => topHoist(m)?.id === role.id);
    if (list.length) {
      groups.push({ key: role.id, title: role.name, members: list });
    }
  }
  const plainOnline = online.filter((m) => !topHoist(m));
  if (plainOnline.length) {
    groups.push({ key: "__online", title: "En ligne", members: plainOnline });
  }

  return (
    <div className="w-60 shrink-0 overflow-y-auto border-l border-line bg-sidebar px-2 py-4 scroll-thin">
      {groups.map((g) => (
        <div key={g.key}>
          <Section title={`${g.title} — ${g.members.length}`} />
          {g.members.map((m) => (
            <MemberRow
              key={m.user.id}
              guildId={guildId}
              name={m.nick || displayName(m.user)}
              id={m.user.id}
              status={presences[m.user.id] ?? "online"}
              avatarId={m.user.avatar_id}
              nameStyle={roleNameStyle(memberTopColorRole(roles, m)) ?? undefined}
            />
          ))}
        </div>
      ))}

      {offline.length > 0 && <Section title={`Hors ligne — ${offline.length}`} />}
      {offline.map((m) => (
        <MemberRow
          key={m.user.id}
          guildId={guildId}
          name={m.nick || displayName(m.user)}
          id={m.user.id}
          status="offline"
          avatarId={m.user.avatar_id}
          nameStyle={roleNameStyle(memberTopColorRole(roles, m)) ?? undefined}
          dim
        />
      ))}
    </div>
  );
}

function Section({ title }: { title: string }) {
  return (
    <div className="mb-1 mt-4 px-2 text-xs font-semibold uppercase tracking-wide text-channel">
      {title}
    </div>
  );
}

function MemberRow({
  guildId,
  name,
  id,
  status,
  avatarId,
  nameStyle,
  dim,
}: {
  guildId: string;
  name: string;
  id: string;
  status?: string;
  avatarId?: string | null;
  nameStyle?: RoleNameStyle;
  dim?: boolean;
}) {
  // Statut personnalisé : affiché sous le nom (mis à jour en direct via PRESENCE_UPDATE).
  const customStatus = useStore((s) => s.customStatus[id]);
  return (
    <UserPopover userId={id} side="left" guildId={guildId}>
      <div
        className={`flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 transition-colors hover:bg-hover ${
          dim ? "opacity-40" : ""
        }`}
      >
        <Avatar name={name} id={id} size={32} status={status ?? "offline"} avatarId={avatarId} />
        <div className="min-w-0 flex-1 leading-tight">
          <div
            className={`truncate text-[15px] text-interactive-normal ${nameStyle?.className ?? ""}`}
            style={nameStyle?.style}
          >
            {name}
          </div>
          {customStatus && <div className="truncate text-xs text-muted">{customStatus}</div>}
        </div>
      </div>
    </UserPopover>
  );
}
