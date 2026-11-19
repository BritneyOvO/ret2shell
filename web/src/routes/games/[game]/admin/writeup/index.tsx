import { useUpdateGameDocMutation } from "@api/game";
import { useWriteups } from "@api/writeup";
import GameDocForm from "@routes/games/[game]/_blocks/doc-form";
import { useParams } from "@solidjs/router";
import { Title } from "@storage/header";
import { t } from "@storage/theme";
import Button from "@widgets/button";
import { createMemo } from "solid-js";

export default function () {
  const params = useParams();
  const gameId = createMemo(() => Number.parseInt(params.game ?? "", 10) || -1);
  const updateMutation = useUpdateGameDocMutation();
  const submissions = useWriteups(gameId, () => gameId() > 0, true);

  async function onSubmit(content: string) {
    await updateMutation.mutateAsync({ id: gameId(), type: "writeup", content });
  }

  return (
    <>
      <Title page={t("game.writeup.adminTitle")} route={`/games/${gameId()}/admin/writeup`} />
      <div class="flex flex-col p-3 lg:p-6 w-full items-center space-y-8">
        <GameDocForm gameId={gameId()} docType="writeup" onDone={onSubmit} />
        <section class="w-full max-w-5xl">
          <h2 class="text-xl font-semibold mb-3">{t("game.writeup.submissions")}</h2>
          <div class="overflow-x-auto">
            <table class="w-full text-left">
              <thead>
                <tr class="border-b border-layer-content/10">
                  <th class="p-2">{t("game.writeup.team")}</th>
                  <th class="p-2">{t("game.writeup.filename")}</th>
                  <th class="p-2">{t("game.writeup.action")}</th>
                </tr>
              </thead>
              <tbody>
                {submissions.data?.map((submission) => (
                  <tr class="border-b border-layer-content/10">
                    <td class="p-2">{submission.team_id}</td>
                    <td class="p-2">{submission.filename}</td>
                    <td class="p-2">
                      <Button
                        type="button"
                        onClick={async () => {
                          const blob = await (await import("@api/writeup")).downloadWriteup(gameId(), submission.id);
                          const url = URL.createObjectURL(blob);
                          const link = document.createElement("a");
                          link.href = url;
                          link.download = submission.filename;
                          link.click();
                          URL.revokeObjectURL(url);
                        }}
                      >
                        {t("game.writeup.download")}
                      </Button>
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
