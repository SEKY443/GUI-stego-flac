import { useEffect, useMemo, useState } from "react";
import {
  encode,
  errorMessage,
  onStage,
  pickCoverAudio,
  pickFlacSavePath,
  pickInputFile,
  planPreview,
} from "../api";
import type {
  CoverMode,
  CoverQuality,
  ChannelsChoice,
  EncodeReportDto,
  PlanArgsDto,
  PlanInfoDto,
} from "../types";
import { emptyPlanArgs } from "../types";
import { PlanFields } from "./PlanFields";
import {
  Banner,
  Button,
  Checkbox,
  Field,
  FilePicker,
  ProgressBar,
  Row,
  Section,
  Select,
  TextInput,
  humanBytes,
  humanDuration,
} from "./ui";

export function EncodeView() {
  const [inputPath, setInputPath] = useState<string | null>(null);
  const [outputPath, setOutputPath] = useState<string | null>(null);
  const [passphrase, setPassphrase] = useState("");
  const [confirmPassphrase, setConfirmPassphrase] = useState("");
  const [noEncrypt, setNoEncrypt] = useState(false);

  const [name, setName] = useState("");
  const [noStoreName, setNoStoreName] = useState(false);
  const [level, setLevel] = useState(19);
  const [fecOverhead, setFecOverhead] = useState(5);
  const [fecSymbolSize, setFecSymbolSize] = useState(256);
  const [channels, setChannels] = useState<ChannelsChoice>("auto");

  const [coverEnabled, setCoverEnabled] = useState(false);
  const [coverPath, setCoverPath] = useState<string | null>(null);
  const [coverQuality, setCoverQuality] = useState<CoverQuality>("auto");
  const [coverMode, setCoverMode] = useState<CoverMode>("cut");
  const [coverAttenuation, setCoverAttenuation] = useState(25);
  const [coverKeepMetadata, setCoverKeepMetadata] = useState(false);

  const [splitEnabled, setSplitEnabled] = useState(false);
  const [splitSizeMib, setSplitSizeMib] = useState(25);

  const [plan, setPlan] = useState<PlanArgsDto>(emptyPlanArgs);
  const [preview, setPreview] = useState<PlanInfoDto | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);

  const [stage, setStage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [overwriteMessage, setOverwriteMessage] = useState<string | null>(null);
  const [report, setReport] = useState<EncodeReportDto | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let cancelled = false;
    planPreview(plan)
      .then((info) => {
        if (!cancelled) {
          setPreview(info);
          setPreviewError(null);
        }
      })
      .catch((err) => {
        if (!cancelled) setPreviewError(errorMessage(err));
      });
    return () => {
      cancelled = true;
    };
  }, [plan]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    onStage("encode", setStage).then((fn) => (unlisten = fn));
    return () => unlisten?.();
  }, []);

  const passphraseMismatch = !noEncrypt && passphrase.length > 0 && passphrase !== confirmPassphrase;
  const canSubmit =
    inputPath !== null &&
    outputPath !== null &&
    !busy &&
    (noEncrypt || (passphrase.length > 0 && !passphraseMismatch)) &&
    (!coverEnabled || coverPath !== null) &&
    (!splitEnabled || splitSizeMib > 0) &&
    !(coverEnabled && splitEnabled);

  const requestBody = useMemo(
    () => ({
      inputPath: inputPath ?? "",
      outputPath: outputPath ?? "",
      passphrase: noEncrypt ? undefined : passphrase,
      name: noStoreName || name.trim() === "" ? undefined : name.trim(),
      noStoreName,
      level,
      fecOverhead,
      fecSymbolSize,
      channels,
      cover:
        coverEnabled && coverPath
          ? {
              path: coverPath,
              quality: coverQuality,
              mode: coverMode,
              attenuationDb: coverAttenuation,
              keepMetadata: coverKeepMetadata,
            }
          : undefined,
      splitSizeBytes: splitEnabled ? Math.round(splitSizeMib * 1024 * 1024) : undefined,
      plan,
      force: false,
    }),
    [
      inputPath,
      outputPath,
      noEncrypt,
      passphrase,
      noStoreName,
      name,
      level,
      fecOverhead,
      fecSymbolSize,
      channels,
      coverEnabled,
      coverPath,
      coverQuality,
      coverMode,
      coverAttenuation,
      coverKeepMetadata,
      splitEnabled,
      splitSizeMib,
      plan,
    ],
  );

  async function runEncode(force: boolean) {
    setBusy(true);
    setError(null);
    setOverwriteMessage(null);
    setReport(null);
    try {
      const result = await encode({ ...requestBody, force });
      setReport(result);
      setStage(null);
    } catch (err) {
      const message = errorMessage(err);
      if (message.includes("already exists")) {
        setOverwriteMessage(message);
      } else {
        setError(message);
      }
      setStage(null);
    } finally {
      setBusy(false);
    }
  }

  async function pickInput() {
    const path = await pickInputFile("Choose a file to hide");
    if (!path) return;
    setInputPath(path);
    if (!outputPath) {
      const parts = path.split(/[/\\]/);
      const base = parts[parts.length - 1];
      setOutputPath(`${path}.flac`);
      void base;
    }
    if (!name) {
      const parts = path.split(/[/\\]/);
      setName(parts[parts.length - 1] ?? "");
    }
  }

  return (
    <div className="mx-auto max-w-3xl space-y-4 pb-12">
      <Field label="File to hide">
        <FilePicker
          value={inputPath}
          placeholder="Choose a file…"
          onPick={pickInput}
        />
      </Field>

      <Field label="Output carrier (.flac)">
        <FilePicker
          value={outputPath}
          placeholder="Choose where to save the carrier…"
          onPick={async () => {
            const path = await pickFlacSavePath(
              inputPath ? `${inputPath.split(/[/\\]/).pop()}.flac` : undefined,
            );
            if (path) setOutputPath(path);
          }}
        />
      </Field>

      <Section title="Encryption">
        <Checkbox
          checked={noEncrypt}
          onChange={setNoEncrypt}
          label="Write an unencrypted carrier (not recommended)"
        />
        {!noEncrypt && (
          <div className="grid grid-cols-2 gap-3">
            <Field label="Passphrase">
              <TextInput
                type="password"
                value={passphrase}
                onChange={(e) => setPassphrase(e.target.value)}
              />
            </Field>
            <Field label="Confirm passphrase">
              <TextInput
                type="password"
                value={confirmPassphrase}
                onChange={(e) => setConfirmPassphrase(e.target.value)}
              />
            </Field>
          </div>
        )}
        {passphraseMismatch && <Banner kind="warning">Passphrases do not match.</Banner>}
        {noEncrypt && (
          <Banner kind="warning">
            Anyone with this file will be able to read the hidden payload.
          </Banner>
        )}
      </Section>

      <Section title="Naming" defaultOpen={false}>
        <Checkbox
          checked={noStoreName}
          onChange={setNoStoreName}
          label="Do not store the filename or detected format (fully anonymous payload)"
        />
        {!noStoreName && (
          <Field label="Stored filename" hint="defaults to the input file's own name">
            <TextInput value={name} onChange={(e) => setName(e.target.value)} />
          </Field>
        )}
      </Section>

      <Section title="Advanced: waveform, compression, FEC, channels" defaultOpen={false}>
        <PlanFields plan={plan} onChange={setPlan} advancedOpen />
        <div className="grid grid-cols-3 gap-3 pt-2">
          <Field label="Compression level" hint="1-22, default 19">
            <TextInput
              type="number"
              min={1}
              max={22}
              value={level}
              onChange={(e) => setLevel(Number(e.target.value))}
            />
          </Field>
          <Field label="FEC repair %" hint="0 disables repair">
            <TextInput
              type="number"
              min={0}
              max={100}
              value={fecOverhead}
              onChange={(e) => setFecOverhead(Number(e.target.value))}
            />
          </Field>
          <Field label="FEC symbol size (bytes)">
            <TextInput
              type="number"
              value={fecSymbolSize}
              onChange={(e) => setFecSymbolSize(Number(e.target.value))}
            />
          </Field>
        </div>
        <Field label="Channels" hint="divides carrier duration, leaves file size alone">
          <Select value={channels} onChange={(v) => setChannels(v as ChannelsChoice)}>
            <option value="auto">auto</option>
            {[1, 2, 3, 4, 5, 6, 7, 8].map((n) => (
              <option key={n} value={String(n)}>
                {n}
              </option>
            ))}
          </Select>
        </Field>
      </Section>

      <Section title="Splitting into volumes" defaultOpen={false}>
        <Checkbox
          checked={splitEnabled}
          onChange={(checked) => {
            setSplitEnabled(checked);
            if (checked) setCoverEnabled(false);
          }}
          label="Split the carrier across several smaller FLAC files"
        />
        {splitEnabled && (
          <Field
            label="Size per volume (MiB)"
            hint="each part is written next to the output with .partI-of-N inserted before the extension; decode locates the rest on its own from any one part"
          >
            <TextInput
              type="number"
              min={1}
              value={splitSizeMib}
              onChange={(e) => setSplitSizeMib(Number(e.target.value))}
            />
          </Field>
        )}
        {splitEnabled && coverEnabled && (
          <Banner kind="warning">Splitting cannot be combined with cover audio; disable one.</Banner>
        )}
      </Section>

      <Section title="Radio mode: hide under audible cover audio" defaultOpen={false}>
        <Checkbox
          checked={coverEnabled}
          onChange={(checked) => {
            setCoverEnabled(checked);
            if (checked) setSplitEnabled(false);
          }}
          label="Hide the carrier under cover audio"
        />
        {coverEnabled && (
          <div className="space-y-3">
            <Field label="Cover audio file" hint="FLAC, WAV, MP3, or MP4/M4A">
              <FilePicker
                value={coverPath}
                placeholder="Choose a recording…"
                onPick={async () => {
                  const path = await pickCoverAudio("Choose cover audio");
                  if (path) setCoverPath(path);
                }}
              />
            </Field>
            <div className="grid grid-cols-2 gap-3">
              <Field label="Cover quality">
                <Select value={coverQuality} onChange={(v) => setCoverQuality(v as CoverQuality)}>
                  <option value="auto">auto — widen for small payloads</option>
                  <option value="telephone">telephone — ~3.4 kHz, cheapest</option>
                  <option value="wide">wide — ~5 kHz</option>
                  <option value="full">full — ~7 kHz, roughly doubles size</option>
                </Select>
              </Field>
              <Field label="Cover mode">
                <Select value={coverMode} onChange={(v) => setCoverMode(v as CoverMode)}>
                  <option value="cut">cut — end when the payload does</option>
                  <option value="spread">spread — stretch to play the cover in full</option>
                </Select>
              </Field>
            </div>
            <Field label="Data attenuation below cover (dB)" hint="default 25">
              <TextInput
                type="number"
                value={coverAttenuation}
                onChange={(e) => setCoverAttenuation(Number(e.target.value))}
              />
            </Field>
            <Checkbox
              checked={coverKeepMetadata}
              onChange={setCoverKeepMetadata}
              label="Copy the cover's own tags (title/artist/album) into the carrier"
            />
          </div>
        )}
      </Section>

      {preview && (
        <Section title="Plan preview" defaultOpen={false}>
          <Row label="Waveform" value={preview.description} />
          <Row label="Sample rate" value={`${preview.sampleRateHz} Hz`} />
          <Row label="Band" value={`${preview.bandHz[0].toFixed(0)}-${preview.bandHz[1].toFixed(0)} Hz`} />
          <Row label="Bit rate" value={`${preview.bitRate.toFixed(0)} bit/s`} />
          <Row
            label="Est. duration for this file's size"
            value={preview.durationForPayload[1] ? humanDuration(preview.durationForPayload[1].durationSecs) : "—"}
          />
        </Section>
      )}
      {previewError && <Banner kind="warning">{previewError}</Banner>}

      <div className="flex items-center gap-3 pt-2">
        <Button onClick={() => runEncode(false)} disabled={!canSubmit}>
          {busy ? "Encoding…" : "Encode"}
        </Button>
        {busy && <ProgressBar stage={stage} />}
      </div>

      {overwriteMessage && (
        <Banner kind="warning">
          <div className="flex items-center justify-between gap-3">
            <span>{overwriteMessage}</span>
            <Button variant="danger" onClick={() => runEncode(true)}>
              Overwrite
            </Button>
          </div>
        </Banner>
      )}
      {error && <Banner kind="error">{error}</Banner>}

      {report && (
        <Section title="Result">
          {report.volumes.length > 0 ? (
            <div className="space-y-1 pb-1">
              <span className="text-xs font-medium uppercase tracking-wide text-neutral-500 dark:text-neutral-400">
                Wrote {report.volumes.length} volumes
              </span>
              {report.volumes.map((v) => (
                <Row
                  key={v.part}
                  label={`${v.part}/${v.of}`}
                  value={`${v.path} (${humanBytes(v.carrierBytes)}, ${humanDuration(v.durationSecs)}, ${v.channels} ch)`}
                />
              ))}
            </div>
          ) : (
            <Row label="Wrote" value={report.outputPath} />
          )}
          <Row label="Input" value={humanBytes(report.plaintextBytes)} />
          <Row
            label="Compressed"
            value={
              report.compressed
                ? `${humanBytes(report.compressed.bytes)} (${(report.compressed.ratio * 100).toFixed(1)}% of original)`
                : "skipped (incompressible)"
            }
          />
          <Row label="Encrypted" value={report.encrypted ? "AES-256-GCM, Argon2id key" : "no"} />
          <Row label="Filename stored" value={report.storedName ?? "no"} />
          <Row label="Format detected" value={report.detectedFormat?.description ?? "unrecognised"} />
          <Row label="FEC" value={`${report.fecPackets} RaptorQ packets (${report.fecRepairPercent}% repair)`} />
          <Row label="Frame" value={`${humanBytes(report.frameBytes)} (${report.expansionRatio.toFixed(2)}x plaintext)`} />
          <Row label="Waveform" value={report.waveformDescription} />
          {report.coverBandHz && (
            <Row
              label="Cover audio"
              value={`${report.coverBandHz[0].toFixed(0)}-${report.coverBandHz[1].toFixed(0)} Hz`}
            />
          )}
          {report.volumes.length === 0 && (
            <>
              <Row
                label="Carrier"
                value={`${humanDuration(report.durationSecs)}${report.channels > 1 ? ` across ${report.channels} channels` : ""}`}
              />
              <Row
                label="FLAC file"
                value={`${humanBytes(report.carrierBytes)} (${report.carrierRatio.toFixed(2)}x plaintext)`}
              />
            </>
          )}
          {report.volumes.length > 0 && (
            <Row
              label="Carrier (total)"
              value={`${humanDuration(report.durationSecs)}, ${humanBytes(report.carrierBytes)} (${report.carrierRatio.toFixed(2)}x plaintext)`}
            />
          )}
          {!report.encrypted && (
            <Banner kind="warning">This carrier is not encrypted; anyone with the file can read it.</Banner>
          )}
        </Section>
      )}
    </div>
  );
}
