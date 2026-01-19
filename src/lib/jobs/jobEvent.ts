export type JobEvent =
  | {
      type: 'Started';
      job_id: string;
      step_id: string;
      at_ms: number;
      title: string;
      command: string;
      args: string[];
    }
  | {
      type: 'Log';
      job_id: string;
      step_id: string;
      at_ms: number;
      level: string;
      message: string;
    }
  | {
      type: 'Status';
      job_id: string;
      step_id: string;
      at_ms: number;
      phase: string;
      key?: string;
      message: string;
    }
  | {
      type: 'Progress';
      job_id: string;
      step_id: string;
      at_ms: number;
      phase: string;
      fraction: number | null;
      eta_ms: number | null;
      speed_bps: number | null;
      downloaded_bytes: number | null;
      total_bytes: number | null;
    }
  | {
      type: 'Artifact';
      job_id: string;
      step_id: string;
      at_ms: number;
      kind: string;
      path: string;
      size_bytes?: number | null;
      ext?: string | null;
    }
  | {
      type: 'Finished';
      job_id: string;
      step_id: string;
      at_ms: number;
      exit_code: number;
    }
  | {
      type: 'Failed';
      job_id: string;
      step_id: string;
      at_ms: number;
      error: string;
    }
  | {
      type: 'Cancelled';
      job_id: string;
      at_ms: number;
      reason: string;
    };

export type JobEventDecodeResult =
  | { ok: true; event: JobEvent }
  | { ok: false; error: string; context?: Record<string, unknown> };

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function isString(value: unknown): value is string {
  return typeof value === 'string';
}

function isFiniteNumber(value: unknown): value is number {
  return typeof value === 'number' && Number.isFinite(value);
}

function isNullableFiniteNumber(value: unknown): value is number | null {
  return value === null || isFiniteNumber(value);
}

function isStringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every(isString);
}

function hasCommonFields(
  obj: Record<string, unknown>
): obj is Record<string, unknown> & { job_id: string; at_ms: number } {
  return isString(obj.job_id) && isFiniteNumber(obj.at_ms);
}

function hasStepFields(
  obj: Record<string, unknown>
): obj is Record<string, unknown> & { step_id: string } {
  return isString(obj.step_id);
}

export function decodeJobEvent(payload: unknown): JobEventDecodeResult {
  if (!isRecord(payload)) {
    return { ok: false, error: 'job-event payload is not an object', context: { payloadType: typeof payload } };
  }

  const type = payload.type;
  if (!isString(type)) {
    return {
      ok: false,
      error: 'job-event payload missing string "type"',
      context: { keys: Object.keys(payload).slice(0, 20) },
    };
  }

  if (!hasCommonFields(payload)) {
    return {
      ok: false,
      error: `job-event(${type}) missing required fields (job_id, at_ms)`,
      context: { job_id: payload.job_id, at_ms: payload.at_ms },
    };
  }

  switch (type) {
    case 'Started': {
      if (!hasStepFields(payload)) return { ok: false, error: 'job-event(Started) missing step_id' };
      if (!isString(payload.title)) return { ok: false, error: 'job-event(Started) missing title' };
      if (!isString(payload.command)) return { ok: false, error: 'job-event(Started) missing command' };
      if (!isStringArray(payload.args)) return { ok: false, error: 'job-event(Started) args is not string[]' };
      return {
        ok: true,
        event: {
          type: 'Started',
          job_id: payload.job_id,
          step_id: payload.step_id,
          at_ms: payload.at_ms,
          title: payload.title,
          command: payload.command,
          args: payload.args,
        },
      };
    }

    case 'Log': {
      if (!hasStepFields(payload)) return { ok: false, error: 'job-event(Log) missing step_id' };
      if (!isString(payload.level)) return { ok: false, error: 'job-event(Log) missing level' };
      if (!isString(payload.message)) return { ok: false, error: 'job-event(Log) missing message' };
      return {
        ok: true,
        event: {
          type: 'Log',
          job_id: payload.job_id,
          step_id: payload.step_id,
          at_ms: payload.at_ms,
          level: payload.level,
          message: payload.message,
        },
      };
    }

    case 'Status': {
      if (!hasStepFields(payload)) return { ok: false, error: 'job-event(Status) missing step_id' };
      if (!isString(payload.phase)) return { ok: false, error: 'job-event(Status) missing phase' };
      if (!(payload.key === undefined || isString(payload.key)))
        return { ok: false, error: 'job-event(Status) key must be string|undefined' };
      if (!isString(payload.message)) return { ok: false, error: 'job-event(Status) missing message' };
      return {
        ok: true,
        event: {
          type: 'Status',
          job_id: payload.job_id,
          step_id: payload.step_id,
          at_ms: payload.at_ms,
          phase: payload.phase,
          key: payload.key,
          message: payload.message,
        },
      };
    }

    case 'Progress': {
      if (!hasStepFields(payload)) return { ok: false, error: 'job-event(Progress) missing step_id' };
      if (!isString(payload.phase)) return { ok: false, error: 'job-event(Progress) missing phase' };
      if (!isNullableFiniteNumber(payload.fraction))
        return { ok: false, error: 'job-event(Progress) fraction must be number|null' };
      if (!isNullableFiniteNumber(payload.eta_ms))
        return { ok: false, error: 'job-event(Progress) eta_ms must be number|null' };
      if (!isNullableFiniteNumber(payload.speed_bps))
        return { ok: false, error: 'job-event(Progress) speed_bps must be number|null' };
      if (!isNullableFiniteNumber(payload.downloaded_bytes))
        return { ok: false, error: 'job-event(Progress) downloaded_bytes must be number|null' };
      if (!isNullableFiniteNumber(payload.total_bytes))
        return { ok: false, error: 'job-event(Progress) total_bytes must be number|null' };

      return {
        ok: true,
        event: {
          type: 'Progress',
          job_id: payload.job_id,
          step_id: payload.step_id,
          at_ms: payload.at_ms,
          phase: payload.phase,
          fraction: payload.fraction,
          eta_ms: payload.eta_ms,
          speed_bps: payload.speed_bps,
          downloaded_bytes: payload.downloaded_bytes,
          total_bytes: payload.total_bytes,
        },
      };
    }

    case 'Artifact': {
      if (!hasStepFields(payload)) return { ok: false, error: 'job-event(Artifact) missing step_id' };
      if (!isString(payload.kind)) return { ok: false, error: 'job-event(Artifact) missing kind' };
      if (!isString(payload.path)) return { ok: false, error: 'job-event(Artifact) missing path' };

      const size_bytes = payload.size_bytes;
      if (!(size_bytes === undefined || isNullableFiniteNumber(size_bytes))) {
        return { ok: false, error: 'job-event(Artifact) size_bytes must be number|null|undefined' };
      }

      const ext = payload.ext;
      if (!(ext === undefined || ext === null || isString(ext))) {
        return { ok: false, error: 'job-event(Artifact) ext must be string|null|undefined' };
      }

      return {
        ok: true,
        event: {
          type: 'Artifact',
          job_id: payload.job_id,
          step_id: payload.step_id,
          at_ms: payload.at_ms,
          kind: payload.kind,
          path: payload.path,
          size_bytes: size_bytes as number | null | undefined,
          ext: ext as string | null | undefined,
        },
      };
    }

    case 'Finished': {
      if (!hasStepFields(payload)) return { ok: false, error: 'job-event(Finished) missing step_id' };
      if (!isFiniteNumber(payload.exit_code))
        return { ok: false, error: 'job-event(Finished) exit_code must be number' };
      return {
        ok: true,
        event: {
          type: 'Finished',
          job_id: payload.job_id,
          step_id: payload.step_id,
          at_ms: payload.at_ms,
          exit_code: payload.exit_code,
        },
      };
    }

    case 'Failed': {
      if (!hasStepFields(payload)) return { ok: false, error: 'job-event(Failed) missing step_id' };
      if (!isString(payload.error)) return { ok: false, error: 'job-event(Failed) missing error' };
      return {
        ok: true,
        event: {
          type: 'Failed',
          job_id: payload.job_id,
          step_id: payload.step_id,
          at_ms: payload.at_ms,
          error: payload.error,
        },
      };
    }

    case 'Cancelled': {
      if (!isString(payload.reason)) return { ok: false, error: 'job-event(Cancelled) missing reason' };
      return {
        ok: true,
        event: {
          type: 'Cancelled',
          job_id: payload.job_id,
          at_ms: payload.at_ms,
          reason: payload.reason,
        },
      };
    }

    default:
      return { ok: false, error: `Unknown job-event type: ${type}`, context: { type } };
  }
}
