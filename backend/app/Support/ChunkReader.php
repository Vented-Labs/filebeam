<?php

declare(strict_types=1);

namespace App\Support;

use App\Models\TransferChunk;
use RuntimeException;
use Throwable;

readonly class ChunkReader
{
    public function __construct(private FilestoreRegistry $stores) {}

    /** @return resource
     * @throws Throwable
     */
    public function read(TransferChunk $chunk)
    {
        foreach ($chunk->locations()->with('filestore')->orderBy('id')->get() as $location) {
            $input = null;
            $output = tmpfile();
            throw_unless(is_resource($output), new RuntimeException('Unable to spool ciphertext download.'));

            try {
                $input = $this->stores->disk($location->filestore)->readStream($location->storage_path);
                if (! is_resource($input)) {
                    throw new RuntimeException('Ciphertext copy is unavailable.');
                }

                // Validate a bounded chunk before sending headers so another replica can be tried safely.
                $bytes = stream_copy_to_stream($input, $output, $chunk->ciphertext_bytes + 1);
                if ($bytes !== $chunk->ciphertext_bytes) {
                    throw new RuntimeException('Ciphertext copy has an invalid size.');
                }
                rewind($output);
                $hash = hash_init('sha256');
                hash_update_stream($hash, $output);
                if (! hash_equals($chunk->checksum, hash_final($hash))) {
                    throw new RuntimeException('Ciphertext copy has an invalid checksum.');
                }
                rewind($output);

                return $output;
            } catch (Throwable) {
                fclose($output);
            } finally {
                if (is_resource($input)) {
                    fclose($input);
                }
            }
        }

        abort(503, 'No valid ciphertext copy is currently available. Please retry.');
    }
}
