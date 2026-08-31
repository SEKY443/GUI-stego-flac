import { useState } from "react";
import { errorMessage, inspect, pickFlacFile } from "../api";
import type { InfoDto, PlanArgsDto } from "../types";
import { emptyPlanArgs } from "../types";
import { PlanFields } from "./PlanFields";
import { Banner, Button, Field, FilePicker, Row, Section, humanBytes } from "./ui";

export function InfoView() {
  const [inputPath, setInputPath] = useState<string | null>(null);
  const [plan, setPlan] = useState<PlanArgsDto>(emptyPlanArgs);
  const [info, setInfo] = useState<InfoDto | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function run(path: string, planOverride: PlanArgsDto) {
    setBusy(true);
    setError(null);
    try {
      setInfo(await inspect(path, planOverride));
    } catch (err) {
      setError(errorMessage(err));
      setInfo(null);
    } finally {
      setBusy(false);
    }
  }

  async function pickInput() {
    const path = await pickFlacFile("Choose a carrier to inspect");
    if (!path) return;
    setInputPath(path);
    await run(path, plan);
  }

  return (
    <div className="mx-auto max-w-3xl space-y-4 pb-12">
      <Field label="Carrier to inspect" hint="never asks for a passphrase">
        <FilePicker value={inputPath} placeholder="Choose a .flac carrier…" onPick={pickInput} />
      </Field>

      <Section title="Tone plan override" defaultOpen={false}>
        <p className="text-xs text-neutral-500 dark:text-neutral-500">
          Only needed if the carrier's metadata was stripped or you want to check a specific
          configuration.
        </p>
        <PlanFields plan={plan} onChange={setPlan} advancedOpen />
        <Button
          variant="secondary"
          onClick={() => inputPath && run(inputPath, plan)}
          disabled={!inputPath || busy}
        >
          Re-inspect with this plan
        </Button>
      </Section>

      {error && <Banner kind="error">{error}</Banner>}

      {info?.volume && (
        <Section title="Split archive volume">
          <Row label="Part" value={`${info.volume.part} of ${info.volume.of}`} />
          <Row label="Archive ID" value={info.volume.archiveId} />
          <Row
            label="Container"
            value={`${info.sampleRateHz} Hz, ${info.channels} channel(s), ${info.samples} samples`}
          />
          <Row label="Duration" value={`${info.durationSecs.toFixed(1)} s`} />
          <Row label="Profile" value={info.profileLabel} />
          <Row label="Waveform" value={`${info.waveformDescription} (${info.bitRate.toFixed(0)} bit/s)`} />
          {!info.planInMetadata && (
            <Banner kind="warning">No plan recorded in metadata — the plan above was assumed.</Banner>
          )}
          <Row label="Volume payload" value={humanBytes(info.volume.volumeBytes)} />
          <Row label="Full frame" value={humanBytes(info.volume.totalFrameBytes)} />
          <Banner kind="info">
            Decoding this file, or any sibling, locates the other {info.volume.of - 1} part(s) and
            reassembles the frame automatically.
          </Banner>
          {info.warnings.map((warning, i) => (
            <Banner key={i} kind="warning">
              {warning}
            </Banner>
          ))}
        </Section>
      )}

      {info && !info.volume && (
        <Section title="Report">
          <Row
            label="Container"
            value={`${info.sampleRateHz} Hz, ${info.channels} channel(s), ${info.samples} samples`}
          />
          <Row label="Duration" value={`${info.durationSecs.toFixed(1)} s`} />
          <Row label="Profile" value={info.profileLabel} />
          <Row label="Waveform" value={`${info.waveformDescription} (${info.bitRate.toFixed(0)} bit/s)`} />
          <Row label="Band" value={`${info.bandHz[0].toFixed(0)}-${info.bandHz[1].toFixed(0)} Hz`} />
          {!info.planInMetadata && (
            <Banner kind="warning">No plan recorded in metadata — the plan above was assumed.</Banner>
          )}
          <Row label="Format version" value={info.formatVersion ?? "—"} />
          <Row label="Payload size" value={humanBytes(info.payloadBytes ?? 0)} />
          <Row label="Compressed" value={info.compressed ? "yes" : "no"} />
          <Row label="Encrypted" value={info.encrypted ? "yes" : "no"} />
          {info.argon2id && (
            <Row
              label="Argon2id"
              value={`m=${info.argon2id.mCostKib} KiB, t=${info.argon2id.tCost}, p=${info.argon2id.pCost}`}
            />
          )}
          <Row
            label="Filename stored"
            value={info.nameStored ? (info.encrypted ? "yes (encrypted)" : "yes") : "no"}
          />
          <Row
            label="Format stored"
            value={info.formatStored ? (info.encrypted ? "yes (encrypted)" : "yes") : "no"}
          />
          <Row label="FEC" value={info.fec ? "yes" : "no"} />
          <Row label="FEC symbol size" value={`${info.fecSymbolSizeBytes ?? 0} B`} />
          <Row label="Frame size" value={humanBytes(info.frameBytes ?? 0)} />
          <Row label="Carried bytes" value={humanBytes(info.carriedBytes ?? 0)} />
          {info.shortByBytes != null && info.frameBytes != null && (
            <Banner kind="warning">
              Carrier is short by {humanBytes(info.shortByBytes)} —{" "}
              {((info.shortByBytes / info.frameBytes) * 100).toFixed(2)}% of the frame is missing.
              RaptorQ repair symbols may still recover it; try Decode.
            </Banner>
          )}
          {info.warnings.map((warning, i) => (
            <Banner key={i} kind="warning">
              {warning}
            </Banner>
          ))}
        </Section>
      )}
    </div>
  );
}
