import { api_root } from "@api";
import { useTeamList } from "@api/team";
import { useUsers } from "@api/user";
import { useWriteups, type Writeup } from "@api/writeup";
import DownloadButton from "@blocks/download-button";
import { useParams, useSearchParams } from "@solidjs/router";
import { Title } from "@storage/header";
import { t } from "@storage/theme";
import Input from "@widgets/input";
import { DateTime } from "luxon";
import { createMemo } from "solid-js";

function submissionDate(value: unknown) {
  if (value === null || value === undefined || value === "") return null;
  const timestamp = Number(value);
  if (!Number.isFinite(timestamp)) return null;
  return timestamp > 100_000_000_000 ? DateTime.fromMillis(timestamp) : DateTime.fromSeconds(timestamp);
}

export default function () {
  const params = useParams();
  const [searchParams, setSearchParams] = useSearchParams();
  const gameId = createMemo(() => Number.parseInt(params.game ?? "", 10) || -1);
  const filter = createMemo(() => (searchParams.filter as string) || "");
  const submissions = useWriteups(gameId, () => gameId() > 0, true);
  const teams = useTeamList({
    game_id: gameId,
    page: () => 1,
    page_size: () => 100,
    order: () => "id",
    filter: () => "",
    enabled: () => gameId() > 0,
  });
  const users = useUsers({
    page: () => 1,
    page_size: () => 100,
    order: () => "id",
    filter: () => null,
    enabled: () => gameId() > 0,
  });
  const latestSubmissions = createMemo(() => {
    const latest = new Map<number, Writeup>();
    for (const submission of submissions.data ?? []) {
      if (!latest.has(submission.team_id)) latest.set(submission.team_id, submission);
    }
    return [...latest.values()].map((submission) => ({
      submission,
      team: teams.data?.[0].find((team) => team.id === submission.team_id),
      user: users.data?.[0].find((user) => user.id === submission.user_id),
    }));
  });
  const filteredSubmissions = createMemo(() => {
    const query = filter().trim().toLowerCase();
    if (!query) return latestSubmissions();
    return latestSubmissions().filter(({ submission, team, user }) =>
      [
        submission.team_id.toString(),
        team?.name ?? "",
        submission.filename,
        user?.account ?? "",
        user?.nickname ?? "",
      ].some((value) => value.toLowerCase().includes(query))
    );
  });

  return (
    <>
      <Title page={t("game.writeup.adminTitle")} route={`/games/${gameId()}/admin/writeup`} />
      <div class="flex flex-col p-3 lg:p-6 w-full items-center">
        <section class="w-full max-w-5xl">
          <div class="flex flex-wrap items-center gap-3 border-b border-layer-content/10 py-2">
            <h2 class="flex-1 text-xl font-semibold">{t("game.writeup.submissions")}</h2>
            <Input
              class="w-full sm:w-80"
              size="sm"
              icon={<span class="shrink-0 icon-[fluent--filter-16-regular] w-5 h-5" />}
              value={filter()}
              placeholder={t("game.writeup.filter")}
              onInput={(event) => setSearchParams({ filter: event.currentTarget.value || undefined })}
            />
          </div>
          <div class="overflow-x-auto">
            <table class="w-full text-left">
              <thead>
                <tr class="border-b border-layer-content/10 text-sm opacity-70">
                  <th class="p-3">{t("game.writeup.teamId")}</th>
                  <th class="p-3">{t("game.writeup.teamName")}</th>
                  <th class="p-3">{t("game.writeup.filename")}</th>
                  <th class="p-3">{t("game.writeup.submitter")}</th>
                  <th class="p-3">{t("game.writeup.uploadTime")}</th>
                  <th class="p-3">{t("game.writeup.action")}</th>
                </tr>
              </thead>
              <tbody>
                {filteredSubmissions().map(({ submission, team, user }) => (
                  <tr class="border-b border-layer-content/10 hover:bg-layer-content/5">
                    <td class="p-3 font-mono text-sm">{submission.team_id}</td>
                    <td class="p-3">{team?.name ?? "-"}</td>
                    <td class="p-3 max-w-80 break-all">{submission.filename}</td>
                    <td class="p-3">{user?.account ?? user?.nickname ?? submission.user_id}</td>
                    <td class="p-3 text-sm opacity-70">
                      {submissionDate(submission.created_at)?.toFormat("yyyy-MM-dd HH:mm") ?? "-"}
                    </td>
                    <td class="p-2">
                      <DownloadButton
                        type="button"
                        square
                        icon="icon-[fluent--arrow-download-20-regular] w-5 h-5"
                        file={submission.filename}
                        url={`${api_root}/game/${gameId()}/writeup/${submission.id}/download`}
                        title={t("game.writeup.download")}
                        aria-label={t("game.writeup.download")}
                      />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </section>
      </div>
    </>
  );
}
