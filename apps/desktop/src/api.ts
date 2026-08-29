import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import type {
  CommandError,
  DecodeReportDto,
  DecodeRequest,
  EncodeReportDto,
  EncodeRequest,
  InfoDto,
  PlanArgsDto,
  PlanInfoDto,
} from "./types";

/** Every Rust command error collapses to one human-readable message. */
export function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error && typeof error === "object" && "message" in error) {
    return String((error as CommandError).message);
  }
  return String(error);
}

export const planPreview = (args: PlanArgsDto): Promise<PlanInfoDto> =>
  invoke("plan_preview", { args });

export const inspect = (path: string, plan: PlanArgsDto): Promise<InfoDto> =>
  invoke("inspect", { path, plan });

export const decode = (request: DecodeRequest): Promise<DecodeReportDto> =>
  invoke("decode", { request });

export const encode = (request: EncodeRequest): Promise<EncodeReportDto> =>
  invoke("encode", { request });

/** Subscribe to `<what>://stage` progress events; returns the unsubscribe fn. */
export function onStage(
  channel: "encode" | "decode",
  onEvent: (stage: string) => void,
): Promise<UnlistenFn> {
  return listen<string>(`${channel}://stage`, (event) => onEvent(event.payload));
}

export async function pickInputFile(title: string): Promise<string | null> {
  const result = await open({ title, multiple: false, directory: false });
  return typeof result === "string" ? result : null;
}

export async function pickFlacFile(title: string): Promise<string | null> {
  const result = await open({
    title,
    multiple: false,
    directory: false,
    filters: [{ name: "FLAC audio", extensions: ["flac"] }],
  });
  return typeof result === "string" ? result : null;
}

export async function pickCoverAudio(title: string): Promise<string | null> {
  const result = await open({
    title,
    multiple: false,
    directory: false,
    filters: [
      { name: "Audio", extensions: ["flac", "wav", "mp3", "mp4", "m4a", "ogg"] },
    ],
  });
  return typeof result === "string" ? result : null;
}

export async function pickSavePath(
  title: string,
  defaultName?: string,
): Promise<string | null> {
  const result = await save({ title, defaultPath: defaultName });
  return result ?? null;
}

export async function pickFlacSavePath(defaultName?: string): Promise<string | null> {
  const result = await save({
    title: "Save carrier as",
    defaultPath: defaultName,
    filters: [{ name: "FLAC audio", extensions: ["flac"] }],
  });
  return result ?? null;
}
