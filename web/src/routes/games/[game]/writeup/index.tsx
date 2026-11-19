import { useGame, useGameDoc, useUpdateGameDocMutation } from "@api/game";
import { api_root } from "@api/index";
import { useSelfTeam } from "@api/team";
import { useSubmitWriteup, useUploadWriteupExample, useWriteupExample, useWriteups } from "@api/writeup";
import Spin from "@assets/animates/spin";
import DownloadButton from "@blocks/download-button";
import GameDocForm from "@routes/games/[game]/_blocks/doc-form";
import { useParams } from "@solidjs/router";
import { isAdminOfGame } from "@storage/game";
import { t } from "@storage/theme";
import Article from "@widgets/article";
import Button from "@widgets/button";
import Dialog from "@widgets/dialog";
import { HTTPError } from "ky";
import { DateTime } from "luxon";
import { createMemo, createSignal, Show } from "solid-js";

function submissionDate(value: unknown) {
  if (value === null || value === undefined || value === "") return null;
  const timestamp = Number(value);
  if (!Number.isFinite(timestamp)) return null;
  return timestamp > 100_000_000_000 ? DateTime.fromMillis(timestamp) : DateTime.fromSeconds(timestamp);
}

export default function () {
  const params = useParams();
  const gameId = createMemo(() => Number.parseInt(params.game ?? "", 10) || -1);
  const game = useGame({ id: gameId, enabled: () => gameId() > 0 });
  const doc = useGameDoc({ id: gameId, type: () => "writeup", enabled: () => gameId() > 0 });
  const updateDoc = useUpdateGameDocMutation();
  const isAdmin = createMemo(() => isAdminOfGame(game.data));
  const [editorOpen, setEditorOpen] = createSignal(false);
  const team = useSelfTeam({ game_id: gameId, enabled: () => gameId() > 0 });
  const submissions = useWriteups(gameId, () => gameId() > 0 && !!team.data);
  const example = useWriteupExample(gameId, () => gameId() > 0);
  const latestSubmission = createMemo(() => submissions.data?.[0]);
  const [file, setFile] = createSignal<File>();
  let fileInput: HTMLInputElement | undefined;
  let exampleInput: HTMLInputElement | undefined;
  const submit = useSubmitWriteup({
    onSuccess: () => {
      setFile(undefined);
      if (fileInput) fileInput.value = "";
      submissions.refetch();
    },
  });
  const uploadExample = useUploadWriteupExample({
    onSuccess: () => {
      example.refetch();
      if (exampleInput) exampleInput.value = "";
    },
  });

  return (
    <main class="w-full max-w-5xl mx-auto px-4 py-6 lg:px-8 lg:py-8">
      <section class="flex flex-col pb-8">
        <div class="flex flex-wrap items-center justify-between gap-3">
          <h1 class="text-2xl font-bold">{t("game.writeup.requirements")}</h1>
          <Show when={isAdmin()}>
            <Dialog
              stretched
              level="primary"
              open={editorOpen()}
              onOpenChange={(details) => setEditorOpen(details.open)}
              btnContent={
                <>
                  <span class="shrink-0 icon-[fluent--edit-20-regular] w-5 h-5" />
                  <span>{t("game.writeup.editRequirements")}</span>
                </>
              }
            >
              <div class="w-full h-[min(70vh,42rem)] flex flex-col gap-4">
                <h2 class="text-xl font-semibold">{t("game.writeup.editRequirements")}</h2>
                <GameDocForm
                  gameId={gameId()}
                  docType="writeup"
                  onDone={async (content) => {
                    await updateDoc.mutateAsync({ id: gameId(), type: "writeup", content });
                    setEditorOpen(false);
                  }}
                />
              </div>
            </Dialog>
          </Show>
        </div>
        <Show
          when={!doc.isLoading}
          fallback={
            <div class="flex justify-center p-8 opacity-60">
              <Spin width={32} height={32} />
            </div>
          }
        >
          <Show when={doc.data} fallback={<p class="py-6 text-sm opacity-60">{t("game.writeup.noRequirements")}</p>}>
            <Article class="self-center" content={doc.data || ""} extra={true} headingAnchors={true} noExtraPaddings />
          </Show>
        </Show>
      </section>
      <section class="flex flex-col gap-4 border-t border-layer-content/10 py-8">
        <div class="space-y-2">
          <h2 class="text-xl font-semibold">{t("game.writeup.submit.title")}</h2>
          <p class="text-sm opacity-70">{t("game.writeup.submit.hint")}</p>
        </div>
        <input
          ref={fileInput}
          type="file"
          class="hidden"
          accept=".pdf,.doc,.docx,application/pdf,application/msword,application/vnd.openxmlformats-officedocument.wordprocessingml.document"
          onChange={(event) => setFile(event.currentTarget.files?.[0])}
        />
        <div class="flex flex-wrap items-center gap-3">
          <Show when={example.data}>
            {(writeupExample) => (
              <DownloadButton
                type="button"
                level="secondary"
                icon="icon-[fluent--arrow-download-20-regular] w-5 h-5"
                file={writeupExample().filename}
                url={`${api_root}/game/${gameId()}/writeup/example/download`}
                withFileName
                title={t("game.writeup.example.download")}
              />
            )}
          </Show>
          <Show when={isAdmin()}>
            <input
              ref={exampleInput}
              type="file"
              class="hidden"
              accept=".pdf,.doc,.docx,application/pdf,application/msword,application/vnd.openxmlformats-officedocument.wordprocessingml.document"
              onChange={(event) => {
                const selected = event.currentTarget.files?.[0];
                event.currentTarget.value = "";
                if (selected) uploadExample.mutate({ gameId: gameId(), file: selected });
              }}
            />
            <Button
              type="button"
              level="secondary"
              disabled={uploadExample.isPending}
              loading={uploadExample.isPending}
              title={t("game.writeup.example.upload")}
              aria-label={t("game.writeup.example.upload")}
              onClick={() => exampleInput?.click()}
            >
              <span class="shrink-0 icon-[fluent--arrow-upload-20-regular] w-5 h-5" />
              <span>{t("game.writeup.example.upload")}</span>
            </Button>
          </Show>
          <Button
            type="button"
            square
            title={t("general.actions.select.title")}
            aria-label={t("general.actions.select.title")}
            disabled={submit.isPending}
            onClick={() => fileInput?.click()}
          >
            <span class="shrink-0 icon-[fluent--document-20-regular] w-5 h-5" />
          </Button>
          <Show when={file()}>{(selected) => <span class="min-w-0 break-all text-sm">{selected().name}</span>}</Show>
          <Button
            type="button"
            level="primary"
            square
            title={t("game.writeup.submit.button")}
            aria-label={t("game.writeup.submit.button")}
            disabled={!team.data || !file() || submit.isPending}
            loading={submit.isPending}
            onClick={() => {
              const selected = file();
              if (selected) submit.mutate({ gameId: gameId(), file: selected });
            }}
          >
            <span class="shrink-0 icon-[fluent--arrow-upload-20-regular] w-5 h-5" />
          </Button>
        </div>
        <Show when={team.error instanceof HTTPError && team.error.response.status === 404}>
          <p class="text-sm text-warning">{t("game.writeup.noTeam")}</p>
        </Show>
      </section>
      <section class="border-t border-layer-content/10 pt-6">
        <h2 class="text-xl font-semibold">{t("game.writeup.history")}</h2>
        <Show when={team.isLoading || submissions.isFetching}>
          <div class="py-6 opacity-60">
            <Spin width={24} height={24} />
          </div>
        </Show>
        <Show when={submissions.isError}>
          <p class="py-6 text-sm text-error">{t("game.writeup.errors.fetch")}</p>
        </Show>
        <Show when={!team.isLoading && !submissions.isFetching && !submissions.isError && !submissions.data?.length}>
          <p class="py-6 text-sm opacity-60">{t("game.writeup.emptyHistory")}</p>
        </Show>
        <Show when={latestSubmission()}>
          {(submission) => (
            <ul class="mt-3 divide-y divide-layer-content/10">
              <li class="flex flex-wrap items-center gap-x-4 gap-y-1 py-3">
                <span class="shrink-0 icon-[fluent--document-text-20-regular] w-5 h-5 opacity-60" />
                <span class="flex-1 min-w-0 break-all">{submission().filename}</span>
                <time
                  class="text-sm opacity-60"
                  datetime={submissionDate(submission().created_at)?.toISO() ?? undefined}
                >
                  {submissionDate(submission().created_at)?.toFormat("yyyy-MM-dd HH:mm") ?? "-"}
                </time>
              </li>
            </ul>
          )}
        </Show>
      </section>
    </main>
  );
}
