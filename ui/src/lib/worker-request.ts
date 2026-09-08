export type WorkerMessage = Record<string, unknown>;

export function waitForWorkerMessage<T extends WorkerMessage>(
    worker: Worker,
    matches: (message: WorkerMessage) => message is T,
    options: {
        timeoutMs?: number;
        timeoutMessage?: string;
        workerErrorMessage: string;
        messageErrorMessage: string;
        onWorkerError?: () => void;
    },
): { promise: Promise<T>; reject: (reason: Error) => void } {
    let rejectRequest!: (reason: Error) => void;
    const promise = new Promise<T>((resolve, reject) => {
        const cleanup = () => {
            if (timeout !== undefined) window.clearTimeout(timeout);
            worker.removeEventListener('message', onMessage);
            worker.removeEventListener('error', onError);
            worker.removeEventListener('messageerror', onMessageError);
        };
        const finish = (reason?: Error, message?: T) => {
            cleanup();
            if (reason) reject(reason);
            else resolve(message!);
        };
        const onMessage = (event: MessageEvent<WorkerMessage>) => {
            if (matches(event.data)) finish(undefined, event.data);
        };
        const onError = () => {
            options.onWorkerError?.();
            finish(new Error(options.workerErrorMessage));
        };
        const onMessageError = () => finish(new Error(options.messageErrorMessage));
        const timeout = options.timeoutMs
            ? window.setTimeout(
                  () => finish(new Error(options.timeoutMessage ?? 'Worker request timed out.')),
                  options.timeoutMs,
              )
            : undefined;
        rejectRequest = (reason) => finish(reason);
        worker.addEventListener('message', onMessage);
        worker.addEventListener('error', onError);
        worker.addEventListener('messageerror', onMessageError);
    });

    return { promise, reject: rejectRequest };
}
