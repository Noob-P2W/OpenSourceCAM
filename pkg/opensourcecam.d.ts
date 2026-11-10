/* tslint:disable */
/* eslint-disable */

export class WebSession {
    free(): void;
    [Symbol.dispose](): void;
    edit(data: string): string;
    history_checkpoint(): string;
    import_file(name: string, data: Uint8Array, inches: boolean, role: string): string;
    machine_profile(): string;
    machine_record(): string;
    constructor(data: string);
    operation_draft(strategy: string): string;
    plan(action: string, options: string): string;
    playback_tool(index: number): string;
    restore_history(data: string): string;
    run(action: string): string;
    save_project(): string;
    simulate(progress: number, resolution: number): string;
    stock_obj(progress: number, resolution: number): string;
    stock_preview(progress: number, resolution: number): string;
    tool_library(): string;
    tool_preview(id: bigint): string;
    view(): string;
}

/**
 * Validate a browser-kernel result through the same Part and Project boundaries as native import.
 */
export function cad_project(name: string, source: string, format: string, mesh: string): string;

/**
 * Menu and editor metadata comes from the same definitions as the desktop UI.
 */
export function catalog(): string;

export function import_model(name: string, data: Uint8Array, inches: boolean): string;

export function machine_records(data: string): string;

export function normalize(data: string): string;

export function operation(): string;

export function run(data: string, action: string): string;

export function sample(): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_websession_free: (a: number, b: number) => void;
    readonly cad_project: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number) => void;
    readonly catalog: (a: number) => void;
    readonly import_model: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly machine_records: (a: number, b: number, c: number) => void;
    readonly normalize: (a: number, b: number, c: number) => void;
    readonly operation: (a: number) => void;
    readonly run: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly sample: (a: number) => void;
    readonly websession_edit: (a: number, b: number, c: number, d: number) => void;
    readonly websession_history_checkpoint: (a: number, b: number) => void;
    readonly websession_import_file: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number) => void;
    readonly websession_machine_profile: (a: number, b: number) => void;
    readonly websession_machine_record: (a: number, b: number) => void;
    readonly websession_new: (a: number, b: number, c: number) => void;
    readonly websession_operation_draft: (a: number, b: number, c: number, d: number) => void;
    readonly websession_plan: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly websession_playback_tool: (a: number, b: number, c: number) => void;
    readonly websession_restore_history: (a: number, b: number, c: number, d: number) => void;
    readonly websession_run: (a: number, b: number, c: number, d: number) => void;
    readonly websession_save_project: (a: number, b: number) => void;
    readonly websession_simulate: (a: number, b: number, c: number, d: number) => void;
    readonly websession_stock_obj: (a: number, b: number, c: number, d: number) => void;
    readonly websession_stock_preview: (a: number, b: number, c: number, d: number) => void;
    readonly websession_tool_library: (a: number, b: number) => void;
    readonly websession_tool_preview: (a: number, b: number, c: bigint) => void;
    readonly websession_view: (a: number, b: number) => void;
    readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
    readonly __wbindgen_export: (a: number, b: number) => number;
    readonly __wbindgen_export2: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_export3: (a: number, b: number, c: number) => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
