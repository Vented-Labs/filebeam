<?php

declare(strict_types=1);

namespace App\Filament\Support;

use App\Enums\ReportStatus;
use App\Enums\TransferStatus;
use App\Enums\UserRole;
use App\Models\AdminAudit;
use App\Models\Plan;
use App\Models\User;
use Illuminate\Support\Number;
use Illuminate\Support\Str;

class AuditPresentation
{
    public static function label(string $action): string
    {
        return match ($action) {
            'file_report.assigned' => 'Assignment changed',
            'file_report.reviewed' => 'Review decision recorded',
            'file_report.reopened' => 'Report reopened',
            'file_report.note_added' => 'Internal note added',
            'transfer.takedown_requested' => 'Upload made unavailable',
            'transfer.cleanup_retried' => 'Cleanup retry queued',
            'user.role_changed' => 'Account permissions changed',
            'user.plan_changed' => 'Account plan changed',
            'user.suspended' => 'Account suspended',
            'user.reinstated' => 'Account reinstated',
            'user.admin_created_via_cli' => 'Administrator created from console',
            'user.admin_granted_via_cli' => 'Administrator access granted from console',
            'plan.updated' => 'Plan settings updated',
            'instance_setting.updated' => 'Instance setting updated',
            default => Str::headline(str_replace('.', ' ', $action)),
        };
    }

    /** @return list<array{field: string, before: string, after: string}> */
    public static function changes(AdminAudit $record): array
    {
        $rows = [];

        foreach ($record->changes ?? [] as $field => $change) {
            if (! in_array($field, [
                'status', 'assigned_to', 'resolution', 'resolved_at', 'role', 'plan_id',
                'suspended_at', 'email', 'username', 'maximum_transfer_bytes', 'maximum_note_bytes',
                'maximum_file_count', 'default_file_retention_hours', 'maximum_file_retention_hours',
                'default_note_retention_hours', 'maximum_note_retention_hours', 'is_active',
            ], true)) {
                continue;
            }

            $before = is_array($change) ? ($change['from'] ?? null) : null;
            $after = is_array($change) ? ($change['to'] ?? null) : $change;
            $rows[] = [
                'field' => match ($field) {
                    'assigned_to' => 'Assignee',
                    'plan_id' => 'Plan',
                    'is_active' => 'Available for assignment',
                    default => Str::headline($field),
                },
                'before' => self::value($field, $before),
                'after' => self::value($field, $after),
            ];
        }

        return $rows;
    }

    private static function value(string $field, mixed $value): string
    {
        if ($value === null || $value === '') {
            return $field === 'assigned_to' ? 'Unassigned' : 'None';
        }

        if (! is_scalar($value)) {
            return '[not displayed]';
        }

        return match ($field) {
            'role' => UserRole::tryFrom((string) $value)?->getLabel() ?? (string) $value,
            'status' => ReportStatus::tryFrom((string) $value)?->getLabel() ?? TransferStatus::tryFrom((string) $value)?->getLabel() ?? (string) $value,
            'assigned_to' => User::query()->whereKey($value)->value('email') ?? 'Former staff member #'.$value,
            'plan_id' => Plan::query()->whereKey($value)->value('name') ?? 'Former plan #'.$value,
            'is_active' => $value ? 'Yes' : 'No',
            'maximum_transfer_bytes', 'maximum_note_bytes' => Number::fileSize((int) $value, precision: 2),
            default => (string) $value,
        };
    }
}
