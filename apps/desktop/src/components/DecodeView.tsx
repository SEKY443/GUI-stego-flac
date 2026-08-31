import { useEffect, useState } from "react";
import { decode, errorMessage, inspect, onStage, pickFlacFile, pickSavePath } from "../api";
import type { DecodeReportDto, InfoDto } from "../types";
import { emptyPlanArgs } from "../types";
import {
  Banner,
  Button,
  Field,
  FilePicker,
  ProgressBar,
  Row,
  Section,
  TextInput,
  humanBytes,
} from "./ui";

export function DecodeView() {
  const [inputPath, setInputPath] = useState<string | null>(null);
  const [probe, setProbe] = useState<InfoDto | null>(null);
  const [probeError, setProbeError] = useState<string | null>(null);
  const [outputPath, setOutputPath] = useState<string | null>(null);
  const [passphrase, setPassphrase] = useState("");

  const [stage, setStage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [needsOverwrite, setNeedsOverwrite] = useState(false);
  const [report, setReport] = useState<DecodeReportDto | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    onStage("decode", setStage).then((fn) => (unlisten = fn));
    return () => unlisten?.();
  }, []);

  async function pickInput() {
    const path = await pickFlacFile("Choose a carrier to decode");
    if (!path) return;
    setInputPath(path);
    setOutputPath(null);
    setReport(null);
    setError(null);
    setProbe(null);
    setProbeError(null);
    try {
      const info = await inspect(path, emptyPlanArgs);
      setProbe(info);
    } catch (err) {
      setProbeError(errorMessage(err));
    }
  }

  async function runDecode(force: boolean) {
    if (!inputPath) return;
    setBusy(true);
    setError(null);
    setNeedsOverwrite(false);
    setReport(null);
    try {
      const result = await decode({
        inputPath,
        outputPath: outputPath ?? undefined,
        passphrase: probe?.encrypted ? passphrase : undefined,
        force,
        plan: emptyPlanArgs,
      });
      setReport(result);
      setStage(null);
    } catch (err) {
      const message = errorMessage(err);
      if (message.includes("already exists")) {
        setNeedsOverwrite(true);
      } else {
        setError(message);
      }
      setStage(null);
    } finally {
      setBusy(false);
    }
  }

  const canSubmit = inputPath !== null && !busy && (!probe?.encrypted || passphrase.length > 0);

  return (
    <div className="mx-auto max-w-3xl space-y-4 pb-12">
      <Field label="Carrier to decode">
        <FilePicker value={inputPath} placeholder="Choose a .flac carrier…" onPick={pickInput} />
      </Field>

      {probeError && <Banner kind="error">{probeError}</Banner>}

      {probe?.volume && (
        <Section title="Split archive volume">
          <Row label="Part" value={`${probe.volume.part} of ${probe.volume.of}`} />
          <Row label="Archive ID" value={probe.volume.archiveId} />
          <Row label="Waveform" value={probe.waveformDescription} />
          <Row label="Duration" value={`${probe.durationSecs.toFixed(1)} s`} />
          <Row label="Volume payload" value={humanBytes(probe.volume.volumeBytes)} />
          <Row label="Full frame" value={humanBytes(probe.volume.totalFrameBytes)} />
          <Banner kind="info">
            Decoding this file locates the other {probe.volume.of - 1} part(s) next to it and
            reassembles the frame automatically.
          </Banner>
        </Section>
      )}

      {probe && !probe.volume && (
        <Section title="Carrier header">
          <Row label="Waveform" value={probe.waveformDescription} />
          <Row label="Profile" value={probe.profileLabel} />
          <Row label="Duration" value={`${probe.durationSecs.toFixed(1)} s`} />
          <Row label="Payload size" value={humanBytes(probe.payloadBytes ?? 0)} />
          <Row label="Compressed" value={probe.compressed ? "yes" : "no"} />
          <Row label="Encrypted" value={probe.encrypted ? "yes" : "no"} />
          <Row label="Filename stored" value={probe.nameStored ? "yes (visible after decode)" : "no"} />
          <Row label="FEC" value={probe.fec ? `yes (${probe.fecSymbolSizeBytes} B symbols)` : "no"} />
          {probe.shortByBytes != null && (
            <Banner kind="warning">
              Carrier is short by {humanBytes(probe.shortByBytes)} — RaptorQ repair symbols may still
              recover it.
            </Banner>
          )}
        </Section>
      )}

      {probe?.encrypted && (
        <Field label="Passphrase">
          <TextInput
            type="password"
            value={passphrase}
            onChange={(e) => setPassphrase(e.target.value)}
          />
        </Field>
      )}

      <Field
        label="Output location"
        hint="leave blank to save next to the carrier, using its stored filename"
      >
        <FilePicker
          value={outputPath}
          placeholder="(use the stored filename)"
          onPick={async () => {
            const path = await pickSavePath("Save recovered file as", probe?.path?.split(/[/\\]/).pop());
            if (path) setOutputPath(path);
          }}
          onClear={outputPath ? () => setOutputPath(null) : undefined}
        />
      </Field>

      <div className="flex items-center gap-3 pt-2">
        <Button onClick={() => runDecode(false)} disabled={!canSubmit}>
          {busy ? "Decoding…" : "Decode"}
        </Button>
        {busy && <ProgressBar stage={stage} />}
      </div>

      {needsOverwrite && (
        <Banner kind="warning">
          <div className="flex items-center justify-between gap-3">
            <span>Output file already exists.</span>
            <Button variant="danger" onClick={() => runDecode(true)}>
              Overwrite
            </Button>
          </div>
        </Banner>
      )}
      {error && <Banner kind="error">{error}</Banner>}

      {report && (
        <Section title="Result">
          <Row label="Recovered to" value={report.outputPath} />
          <Row label="Recovered bytes" value={humanBytes(report.recoveredBytes)} />
          <Row label="Name" value={report.name ?? "(not stored)"} />
          <Row label="Format" value={report.format?.description ?? "unrecognised"} />
          <Row
            label="Encoded at"
            value={report.encodedAtUnix ? new Date(report.encodedAtUnix * 1000).toLocaleString() : "(not stored)"}
          />
          {report.volumesJoined != null && (
            <Row label="Split archive" value={`reassembled from ${report.volumesJoined} volumes`} />
          )}
          {report.warnings.map((warning, i) => (
            <Banner key={i} kind="warning">
              {warning}
            </Banner>
          ))}
        </Section>
      )}
    </div>
  );
}
