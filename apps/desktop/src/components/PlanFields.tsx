import type { PlanArgsDto } from "../types";
import { Field, Select, TextInput } from "./ui";

/** Turn an optional-number form field into `PlanArgsDto[key]`, `undefined` when blank. */
function num(value: string): number | undefined {
  if (value.trim() === "") return undefined;
  const n = Number(value);
  return Number.isFinite(n) ? n : undefined;
}

/**
 * Profile picker plus the same per-waveform advanced overrides
 * `stego-flac`'s `PlanArgs` exposes (`--fft-size`, `--qam-bits`, `--top-bin`
 * for OFDM; `--samples-per-symbol`, `--bits-per-symbol`, `--bin-spacing` for
 * FSK; `--sample-rate`/`--amplitude`/`--base-bin` for both). Shared by the
 * encode form's Advanced panel and the standalone plan explorer so the two
 * never drift apart.
 */
export function PlanFields({
  plan,
  onChange,
  advancedOpen,
}: {
  plan: PlanArgsDto;
  onChange: (plan: PlanArgsDto) => void;
  advancedOpen: boolean;
}) {
  const set = (patch: Partial<PlanArgsDto>) => onChange({ ...plan, ...patch });
  const isFsk = plan.profile === "standard" || plan.profile === "fast";

  return (
    <div className="space-y-3">
      <Field label="Profile" hint="dense is the default: OFDM, 4096-QAM, 128 kbit/s">
        <Select
          value={plan.profile ?? "dense"}
          onChange={(value) => set({ profile: value === "" ? undefined : (value as PlanArgsDto["profile"]) })}
        >
          <option value="dense">dense — OFDM, 4096-QAM, 128 kbit/s</option>
          <option value="compact">compact — OFDM, 65536-QAM, 170 kbit/s</option>
          <option value="standard">standard — 16-FSK, 2 kbit/s (readable spectrogram)</option>
          <option value="fast">fast — 4-FSK, 4 kbit/s</option>
        </Select>
      </Field>

      {advancedOpen && (
        <div className="grid grid-cols-2 gap-3 pt-1">
          <Field label="Sample rate (Hz)" hint="default 24000">
            <TextInput
              type="number"
              placeholder="24000"
              value={plan.sampleRate ?? ""}
              onChange={(e) => set({ sampleRate: num(e.target.value) })}
            />
          </Field>
          <Field label="Amplitude" hint="peak, normalised full scale">
            <TextInput
              type="number"
              step="0.01"
              placeholder="0.25"
              value={plan.amplitude ?? ""}
              onChange={(e) => set({ amplitude: num(e.target.value) })}
            />
          </Field>
          <Field label="Base bin" hint="lowest data-bearing bin">
            <TextInput
              type="number"
              value={plan.baseBin ?? ""}
              onChange={(e) => set({ baseBin: num(e.target.value) })}
            />
          </Field>

          {isFsk ? (
            <>
              <Field label="Samples per symbol" hint="FSK only">
                <TextInput
                  type="number"
                  value={plan.samplesPerSymbol ?? ""}
                  onChange={(e) => set({ samplesPerSymbol: num(e.target.value) })}
                />
              </Field>
              <Field label="Bits per symbol" hint="FSK only, must divide 8">
                <TextInput
                  type="number"
                  value={plan.bitsPerSymbol ?? ""}
                  onChange={(e) => set({ bitsPerSymbol: num(e.target.value) })}
                />
              </Field>
              <Field label="Bin spacing" hint="FSK only">
                <TextInput
                  type="number"
                  value={plan.binSpacing ?? ""}
                  onChange={(e) => set({ binSpacing: num(e.target.value) })}
                />
              </Field>
            </>
          ) : (
            <>
              <Field label="FFT size" hint="OFDM only, samples per symbol">
                <TextInput
                  type="number"
                  value={plan.fftSize ?? ""}
                  onChange={(e) => set({ fftSize: num(e.target.value) })}
                />
              </Field>
              <Field label="QAM bits" hint="OFDM only, 2-20, even; default 12">
                <TextInput
                  type="number"
                  value={plan.qamBits ?? ""}
                  onChange={(e) => set({ qamBits: num(e.target.value) })}
                />
              </Field>
              <Field label="Top bin" hint="OFDM only, highest data-bearing bin">
                <TextInput
                  type="number"
                  value={plan.topBin ?? ""}
                  onChange={(e) => set({ topBin: num(e.target.value) })}
                />
              </Field>
            </>
          )}
        </div>
      )}
    </div>
  );
}
