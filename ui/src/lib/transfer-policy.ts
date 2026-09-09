import type { FilebeamConfig, TransferDriver, TransferLimits } from '../types';

const unavailableLimits: TransferLimits = {
    maximum_transfer_bytes: null,
    maximum_file_count: null,
    maximum_note_bytes: null,
};

export function transferLimits(config: FilebeamConfig, driver: TransferDriver): TransferLimits {
    if (driver === 'http') {
        return (
            config.transport_policy?.limits.http ?? {
                maximum_transfer_bytes: config.maximum_transfer_bytes,
                maximum_file_count: config.maximum_file_count,
                maximum_note_bytes: config.maximum_note_bytes,
            }
        );
    }

    // A server that has not advertised WebRTC must not accidentally enable it.
    return config.transport_policy?.limits.webrtc ?? unavailableLimits;
}

export function enabledTransferDrivers(config: FilebeamConfig): TransferDriver[] {
    const policy = config.transport_policy;
    if (!policy) return ['http'];
    return policy.enabled_drivers.filter(
        (driver) => driver === 'http' || policy.limits.webrtc !== undefined,
    );
}
