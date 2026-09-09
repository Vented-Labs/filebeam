<?php

declare(strict_types=1);

namespace App\Http\Resources;

use App\Models\Transfer;
use App\Support\ChunkStaging;
use Illuminate\Http\Request;
use Illuminate\Http\Resources\Json\JsonResource;

class TransferResource extends JsonResource
{
    /** @var Transfer */
    public $resource;

    /**
     * Transform the resource into an array.
     *
     * @return array<string, mixed>
     */
    public function toArray(Request $request): array
    {
        return [
            'id' => $this->resource->id,
            'kind' => $this->resource->kind->value,
            'driver' => $this->resource->driver->value,
            'status' => $this->resource->status->value,
            'protocol_version' => $this->resource->protocol_version,
            'chunk_bytes' => $this->resource->chunk_bytes,
            'upload_transport' => $this->resource->driver->value === 'http' ? app(ChunkStaging::class)->transport($this->resource->chunk_bytes) : null,
            'download_concurrency' => config('filebeam.transfers.download_concurrency'),
            'retention_hours' => $this->resource->retention_hours,
            'burn_on_read' => $this->resource->burn_on_read,
            'encrypted_manifest' => $this->resource->encrypted_manifest,
            'encrypted_descriptor' => $this->resource->encrypted_descriptor,
            'declared_ciphertext_bytes' => $this->resource->declared_ciphertext_bytes,
            'ciphertext_bytes' => $this->resource->ciphertext_bytes,
            'expires_at' => $this->resource->expires_at->toIso8601String(),
            'items' => $this->resource->items->map(fn ($item): array => [
                'id' => $item->id,
                'position' => $item->position,
                'chunk_count' => $item->chunk_count,
                'declared_ciphertext_bytes' => $item->declared_ciphertext_bytes,
                'ciphertext_bytes' => $item->ciphertext_bytes,
            ])->all(),
        ];
    }
}
