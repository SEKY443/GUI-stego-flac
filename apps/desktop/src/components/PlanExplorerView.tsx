import { useEffect, useState } from "react";
import { errorMessage, planPreview } from "../api";
import type { PlanArgsDto, PlanInfoDto } from "../types";
import { emptyPlanArgs } from "../types";
import { PlanFields } from "./PlanFields";
import { Banner, Row, Section, humanBytes, humanDuration } from "./ui";

export function PlanExplorerView() {
  const [plan, setPlan] = useState<PlanArgsDto>(emptyPlanArgs);
  const [info, setInfo] = useState<PlanInfoDto | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    planPreview(plan)
      .then((result) => {
        if (!cancelled) {
          setInfo(result);
          setError(null);
        }
      })
      .catch((err) => {
        if (!cancelled) setError(errorMessage(err));
      });
    return () => {
      cancelled = true;
    };
  }, [plan]);

  return (
    <div className="mx-auto max-w-3xl space-y-4 pb-12">
      <Section title="Tone plan">
        <PlanFields plan={plan} onChange={setPlan} advancedOpen />
      </Section>

      {error && <Banner kind="error">{error}</Banner>}

      {info && (
        <>
          <Section title={info.description}>
            <Row label="Sample rate" value={`${info.sampleRateHz} Hz`} />
            <Row label="Occupied band" value={`${info.bandHz[0].toFixed(0)}-${info.bandHz[1].toFixed(0)} Hz`} />
            <Row label="Amplitude" value={`${info.amplitude.toFixed(2)} full scale`} />
            <Row label="Throughput" value={`${info.bitRate.toFixed(0)} bit/s (${humanBytes(info.bitRate / 8)}/s)`} />
            <Row
              label="Carrier expansion"
              value={`${info.carrierExpansionRatio.toFixed(2)}x raw PCM per payload byte`}
            />
          </Section>

          <Section title="Carrier duration for a given payload (before compression)">
            {info.durationForPayload.map((entry) => (
              <Row
                key={entry.payloadBytes}
                label={humanBytes(entry.payloadBytes)}
                value={humanDuration(entry.durationSecs)}
              />
            ))}
          </Section>

          <Section title="Presets" defaultOpen={false}>
            {info.presets.map((preset) => (
              <Row
                key={preset.name}
                label={preset.name}
                value={`${preset.bitRate.toFixed(0)} bit/s — ${preset.description}`}
              />
            ))}
          </Section>
        </>
      )}
    </div>
  );
}
