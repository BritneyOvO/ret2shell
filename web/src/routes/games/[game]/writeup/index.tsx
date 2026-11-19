import { useGameDoc } from "@api/game";
import { useSubmitWriteup, useWriteups } from "@api/writeup";
import { useParams } from "@solidjs/router";
import { Title } from "@storage/header";
import { t } from "@storage/theme";
import Article from "@widgets/article";
import Button from "@widgets/button";
import { createMemo, createSignal, Show } from "solid-js";

export default function () {
  const params = useParams();
  const gameId = createMemo(() => Number.parseInt(params.game ?? "", 10) || -1);
  const doc = useGameDoc({ id: gameId, type: () => "writeup", enabled: () => gameId() > 0 });
  const submissions = useWriteups(gameId, () => gameId() > 0);
  const [file, setFile] = createSignal<File>();
  const submit = useSubmitWriteup({
    onSuccess: () => {
      setFile(undefined);
      submissions.refetch();
    },
  });

  return (
    <>
      <Title page={t("game.writeup.title")} route={`/games/${gameId()}/writeup`} />
      <main class="flex flex-col gap-6 p-4 lg:p-8 max-w-5xl w-full mx-auto">
        <Article content={doc.data || ""} />
        <section class="rounded-lg border border-layer-content/10 p-4 space-y-3">
          <h2 class="text-xl font-semibold">{t("game.writeup.submit.title")}</h2>
          <p class="text-sm opacity-70">{t("game.writeup.submit.hint")}</p>
          <input
            type="file"
            accept=".pdf,.doc,.docx,application/pdf,application/msword,application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            onChange={(event) => setFile(event.currentTarget.files?.[0])}
          />
          <Button
            type="button"
            level="primary"
            disabled={!file() || submit.isPending}
            loading={submit.isPending}
            onClick={() => {
              const selected = file();
              if (selected) submit.mutate({ gameId: gameId(), file: selected });
            }}
          >
            {t("game.writeup.submit.button")}
          </Button>
          <Show when={submissions.data?.length}>
            <p class="text-sm opacity-70">
              {t("game.writeup.submit.previous", { filename: submissions.data?.[0]?.filename ?? "" })}
            </p>
          </Show>
        </section>
      </main>
    </>
  );
}
