// Mirrors the DTOs in crates/audio-modem-gui/src/commands/*.rs. Field names
// match the `#[serde(rename_all = "camelCase")]` Rust structs exactly so no
// translation layer is needed at the IPC boundary.

export type Profile = "dense" | "compact" | "standard" | "fast";
export type ChannelsChoice = "auto" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8";
export type CoverQuality = "auto" | "telephone" | "wide" | "full";
export type CoverMode = "cut" | "spread";

export interface PlanArgsDto {
  profile?: Profile;
  sampleRate?: number;
  amplitude?: number;
  samplesPerSymbol?: number;
  bitsPerSymbol?: number;
  binSpacing?: number;
  fftSize?: number;
  qamBits?: number;
  topBin?: number;
  baseBin?: number;
}

export const emptyPlanArgs: PlanArgsDto = {};

export interface DurationEntry {
  payloadBytes: number;
  durationSecs: number;
}

export interface PresetEntry {
  name: string;
  bitRate: number;
  description: string;
}

export interface PlanInfoDto {
  description: string;
  sampleRateHz: number;
  bandHz: [number, number];
  amplitude: number;
  mode: "fsk" | "ofdm";
  bitRate: number;
  carrierExpansionRatio: number;
  durationForPayload: DurationEntry[];
  presets: PresetEntry[];
}

export interface Argon2Dto {
  mCostKib: number;
  tCost: number;
  pCost: number;
}

export interface InfoDto {
  path: string;
  sampleRateHz: number;
  channels: number;
  samples: number;
  durationSecs: number;
  profileLabel: string;
  planInMetadata: boolean;
  waveformDescription: string;
  bitRate: number;
  bandHz: [number, number];
  formatVersion: number;
  payloadBytes: number;
  compressed: boolean;
  encrypted: boolean;
  argon2id: Argon2Dto | null;
  nameStored: boolean;
  formatStored: boolean;
  fec: boolean;
  fecSymbolSizeBytes: number;
  frameBytes: number;
  carriedBytes: number;
  shortByBytes: number | null;
  warnings: string[];
}

export interface FormatDto {
  id: string;
  extension: string;
  description: string;
}

export interface DecodeRequest {
  inputPath: string;
  outputPath?: string;
  passphrase?: string;
  force: boolean;
  plan: PlanArgsDto;
}

export interface DecodeReportDto {
  outputPath: string;
  recoveredBytes: number;
  name: string | null;
  format: FormatDto | null;
  encodedAtUnix: number | null;
  warnings: string[];
}

export interface CoverOptions {
  path: string;
  quality: CoverQuality;
  mode: CoverMode;
  attenuationDb: number;
  keepMetadata: boolean;
}

export interface EncodeRequest {
  inputPath: string;
  outputPath: string;
  passphrase?: string;
  name?: string;
  noStoreName: boolean;
  level: number;
  fecOverhead: number;
  fecSymbolSize: number;
  channels: ChannelsChoice;
  cover?: CoverOptions;
  plan: PlanArgsDto;
  force: boolean;
}

export interface CompressedDto {
  bytes: number;
  ratio: number;
}

export interface EncodeReportDto {
  outputPath: string;
  plaintextBytes: number;
  compressed: CompressedDto | null;
  encrypted: boolean;
  storedName: string | null;
  detectedFormat: FormatDto | null;
  fecPackets: number;
  fecRepairPercent: number;
  frameBytes: number;
  expansionRatio: number;
  waveformDescription: string;
  bitRate: number;
  bandHz: [number, number];
  coverBandHz: [number, number] | null;
  channels: number;
  channelsAuto: boolean;
  durationSecs: number;
  carrierBytes: number;
  carrierRatio: number;
}

export interface CommandError {
  message: string;
}
