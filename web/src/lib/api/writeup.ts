import { t } from "@storage/theme";
import { useMutation, useQuery } from "@tanstack/solid-query";
import { createMemo } from "solid-js";
import api, { api_root, handleHttpError, inflyClient, toastSuccess } from ".";

export type Writeup = {
  id: number;
  game_id: number;
  team_id: number;
  user_id: number;
  media_hash: string;
  filename: string;
  content_type: string;
  created_at: number;
};

export async function getWriteups(gameId: number) {
  return await api.get(`${api_root}/game/${gameId}/writeup`).json<Writeup[]>();
}

export async function getAllWriteups(gameId: number) {
  return await api.get(`${api_root}/game/${gameId}/writeup/admin`).json<Writeup[]>();
}

export function useWriteups(gameId: () => number, enabled?: () => boolean, admin = false) {
  const keys = createMemo(() => ["game", gameId(), "writeups", admin]);
  return useQuery(
    () => ({
      queryKey: keys(),
      queryFn: () => (admin ? getAllWriteups(gameId()) : getWriteups(gameId())),
      enabled: enabled?.(),
      throwOnError: (error: Error) => {
        handleHttpError(error, t("game.writeup.errors.fetch"));
        return false;
      },
    }),
    () => inflyClient
  );
}

export async function submitWriteup(gameId: number, file: File) {
  const form = new FormData();
  form.append("file", file, file.name);
  return await api.post(`${api_root}/game/${gameId}/writeup`, { body: form }).json<Writeup>();
}

export function useSubmitWriteup(props: { onSuccess?: () => void } = {}) {
  return useMutation(() => ({
    mutationFn: ({ gameId, file }: { gameId: number; file: File }) => submitWriteup(gameId, file),
    onSuccess: () => {
      toastSuccess(t("game.writeup.submit.success"));
      props.onSuccess?.();
    },
    onError: (error: Error) => handleHttpError(error, t("game.writeup.submit.fail")),
  }));
}

export async function downloadWriteup(gameId: number, writeupId: number) {
  return await api.get(`${api_root}/game/${gameId}/writeup/${writeupId}/download`).blob();
}
