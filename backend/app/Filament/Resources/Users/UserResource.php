<?php

declare(strict_types=1);

namespace App\Filament\Resources\Users;

use App\Actions\Admin\ManageUser;
use App\Enums\UserRole;
use App\Filament\Resources\Plans\PlanResource;
use App\Filament\Resources\Users\Pages\ListUsers;
use App\Filament\Resources\Users\Pages\ViewUser;
use App\Filament\Resources\Users\RelationManagers\OwnedTransfersRelationManager;
use App\Filament\Support\AuditPresentation;
use App\Models\AdminAudit;
use App\Models\Plan;
use App\Models\User;
use Filament\Actions\Action;
use Filament\Actions\ActionGroup;
use Filament\Facades\Filament;
use Filament\Forms\Components\Placeholder;
use Filament\Forms\Components\Select;
use Filament\Forms\Components\Textarea;
use Filament\Forms\Components\TextInput;
use Filament\Infolists\Components\TextEntry;
use Filament\Notifications\Notification;
use Filament\Resources\Pages\PageRegistration;
use Filament\Resources\Resource;
use Filament\Schemas\Components\Section;
use Filament\Schemas\Components\Utilities\Get;
use Filament\Schemas\Schema;
use Filament\Support\Icons\Heroicon;
use Filament\Tables\Columns\IconColumn;
use Filament\Tables\Columns\TextColumn;
use Filament\Tables\Filters\Filter;
use Filament\Tables\Filters\SelectFilter;
use Filament\Tables\Table;
use Illuminate\Database\Eloquent\Builder;

class UserResource extends Resource
{
    protected static ?string $model = User::class;

    protected static ?string $recordTitleAttribute = 'email';

    protected static string|\BackedEnum|null $navigationIcon = Heroicon::OutlinedUsers;

    protected static string|\UnitEnum|null $navigationGroup = 'Administration';

    protected static ?int $navigationSort = 30;

    public static function table(Table $table): Table
    {
        return $table
            ->recordUrl(fn (User $record): string => static::getUrl('view', ['record' => $record]))
            ->columns([
                TextColumn::make('email')->searchable()->sortable(),
                TextColumn::make('username')->searchable(),
                TextColumn::make('role')->badge(),
                IconColumn::make('email_verified_at')->label('Verified')->boolean()->state(fn (User $record): bool => $record->email_verified_at !== null),
                TextColumn::make('plan.name')->label('Plan'),
                TextColumn::make('suspended_at')->label('Suspended')->dateTime(),
            ])
            ->filters([
                SelectFilter::make('role')
                    ->options(collect(UserRole::cases())->mapWithKeys(fn (UserRole $role): array => [$role->value => $role->name])),
                Filter::make('verified')
                    ->query(fn (Builder $query): Builder => $query->whereNotNull('email_verified_at')),
                SelectFilter::make('plan')->relationship('plan', 'name'),
            ])
            ->recordActions([
                ActionGroup::make([
                    self::changeRoleAction(),
                    self::changePlanAction(),
                    self::suspensionAction(),
                ])->label('Manage'),
            ]);
    }

    /** @return array<string> */
    public static function getGloballySearchableAttributes(): array
    {
        return ['email', 'username'];
    }

    public static function getEloquentQuery(): Builder
    {
        return parent::getEloquentQuery()->with('plan');
    }

    public static function infolist(Schema $schema): Schema
    {
        return $schema->schema([
            Section::make('Account summary')
                ->columnSpanFull()
                ->schema([
                    TextEntry::make('name')->placeholder('Not provided'),
                    TextEntry::make('username')->placeholder('Not provided'),
                    TextEntry::make('email')->copyable(),
                    TextEntry::make('role')->badge(),
                    TextEntry::make('email_verified_at')->label('Email verification')->dateTime()->placeholder('Not verified'),
                    TextEntry::make('suspended_at')->label('Account status')->state(fn (User $record): string => $record->suspended_at === null ? 'Active' : 'Suspended since '.$record->suspended_at->toDayDateTimeString())->badge(),
                    TextEntry::make('effective_plan')->label('Effective plan')->state(fn (User $record): string => self::planSummary($record->plan, $record->plan_id === null)),
                    TextEntry::make('created_at')->label('Joined')->dateTime(),
                ])->columns(['default' => 1, 'sm' => 2, 'xl' => 4]),
            Section::make('Transfer activity')
                ->columnSpanFull()
                ->description('Owned transfers only. Received transfers are not shown because this account does not have a delivery inbox summary.')
                ->schema([
                    TextEntry::make('owned_transfers_count')->label('Owned transfers')->state(fn (User $record): int => $record->ownedTransfers()->count())->numeric(),
                    TextEntry::make('owned_completed_transfers_count')->label('Completed')->state(fn (User $record): int => $record->ownedTransfers()->whereNotNull('completed_at')->count())->numeric(),
                    TextEntry::make('owned_active_transfers_count')->label('Available now')->state(fn (User $record): int => $record->ownedTransfers()->availableAndUnexpired()->count())->numeric(),
                ])->columns(['default' => 1, 'sm' => 3]),
            Section::make('Account activity')
                ->columnSpanFull()
                ->schema([
                    TextEntry::make('account_activity')
                        ->hiddenLabel()
                        ->state(fn (User $record): array => AdminAudit::query()
                            ->where('target_type', User::class)
                            ->where('target_id', (string) $record->getKey())
                            ->with('actor:id,email')
                            ->latest()
                            ->limit(10)
                            ->get()
                            ->map(fn (AdminAudit $audit): string => sprintf('%s | %s | %s%s', $audit->created_at?->toDayDateTimeString() ?? 'Unknown time', AuditPresentation::label($audit->action), $audit->actor === null ? 'System / former staff' : $audit->actor->email, filled($audit->reason) ? ' | '.$audit->reason : ''))
                            ->all())
                        ->listWithLineBreaks()
                        ->placeholder('No administrative account activity recorded.'),
                ]),
        ]);
    }

    /** @return array<string, PageRegistration> */
    public static function getPages(): array
    {
        return [
            'index' => ListUsers::route('/'),
            'view' => ViewUser::route('/{record}'),
        ];
    }

    /** @return array<class-string> */
    public static function getRelations(): array
    {
        return [OwnedTransfersRelationManager::class];
    }

    public static function changeRoleAction(): Action
    {
        return Action::make('changeRole')
            ->label('Change role')
            ->slideOver()
            ->modalDescription('Roles control what staff permissions this account receives. A reason is recorded in the administrative audit history.')
            ->authorize('update')
            ->schema([
                Placeholder::make('current_role')->label('Current role')->content(fn (User $record): string => $record->role->getLabel()),
                Select::make('role')->label('Proposed role')->options(self::roleOptions())->required()->live(),
                Placeholder::make('role_consequence')->label('Effect')->content(fn (Get $get): string => match ($get('role')) {
                    UserRole::Admin->value => 'Administrators can manage accounts and plans.',
                    UserRole::Moderator->value => 'Moderators can access moderation tools but cannot manage accounts or plans.',
                    default => 'Users cannot access the administration panel.',
                }),
                self::reasonField(),
            ])
            ->fillForm(fn (User $record): array => ['role' => $record->role->value])
            ->action(function (array $data, User $record, ManageUser $manager): void {
                $manager->changeRole(self::actor(), $record, UserRole::from($data['role']), $data['reason']);
                Notification::make()->success()->title('Role updated')->body('The account role and staff access have been updated.')->send();
            });
    }

    public static function createUserAction(): Action
    {
        return Action::make('createUser')
            ->label('Create user')
            ->authorize('create')
            ->schema(self::userFields())
            ->action(function (array $data, ManageUser $manager): void {
                $manager->create(self::actor(), $data);
                Notification::make()->success()->title('User created')->body('The user can sign in and must verify their email.')->send();
            });
    }

    public static function inviteUserAction(): Action
    {
        return Action::make('inviteUser')
            ->label('Invite user')
            ->authorize('create')
            ->schema([
                TextInput::make('email')->email()->required()->maxLength(255),
            ])
            ->action(function (array $data, ManageUser $manager): void {
                $manager->invite(self::actor(), $data);
                Notification::make()->success()->title('Invitation sent')->body('The invitation expires in 72 hours. Sending another invitation replaces its link.')->send();
            });
    }

    public static function changePlanAction(): Action
    {
        return Action::make('changePlan')
            ->label('Change plan')
            ->slideOver()
            ->modalDescription('The new plan applies to relevant future transfers. Existing transfer snapshots are not changed.')
            ->authorize('update')
            ->schema([
                Placeholder::make('current_plan')->label('Current plan')->content(fn (User $record): string => self::planSummary($record->plan, $record->plan_id === null)),
                Select::make('plan_id')->label('Proposed plan')->options(fn (): array => Plan::query()->active()->orderBy('name')->pluck('name', 'id')->all())->searchable()->nullable()->live(),
                Placeholder::make('plan_consequence')->label('Selected plan preview')->content(fn (Get $get): string => self::planPreview($get('plan_id'))),
                self::reasonField(),
            ])
            ->fillForm(fn (User $record): array => ['plan_id' => $record->plan_id])
            ->action(function (array $data, User $record, ManageUser $manager): void {
                $manager->changePlan(self::actor(), $record, filled($data['plan_id']) ? Plan::query()->whereKey($data['plan_id'])->firstOrFail() : null, $data['reason']);
                Notification::make()->success()->title('Plan updated')->body('The effective plan for future relevant transfers has been updated.')->send();
            });
    }

    public static function suspensionAction(): Action
    {
        return Action::make('suspend')
            ->label(fn (User $record): string => $record->suspended_at === null ? 'Suspend' : 'Reinstate')
            ->color(fn (User $record): string => $record->suspended_at === null ? 'danger' : 'success')
            ->slideOver()
            ->modalDescription(fn (User $record): string => $record->suspended_at === null ? 'Suspending blocks account login and ends ordinary account access. Existing uploads are not removed automatically. A reason is required and recorded.' : 'Reinstating restores ordinary account access. Staff panel access still depends on role and email verification. Existing uploads remain unchanged. A reason is required and recorded.')
            ->authorize('update')
            ->schema([
                Placeholder::make('current_status')->label('Current status')->content(fn (User $record): string => $record->suspended_at === null ? 'Active' : 'Suspended'),
                Placeholder::make('proposed_status')->label('Proposed status')->content(fn (User $record): string => $record->suspended_at === null ? 'Suspended' : 'Active'),
                self::reasonField(),
            ])
            ->action(function (array $data, User $record, ManageUser $manager): void {
                $suspended = $record->suspended_at === null;
                $manager->setSuspension(self::actor(), $record, $suspended, $data['reason']);
                Notification::make()->success()->title($suspended ? 'Account suspended' : 'Account reinstated')->body($suspended ? 'Account access is blocked; existing uploads were not removed.' : 'Ordinary account access has been restored.')->send();
            });
    }

    private static function reasonField(): Textarea
    {
        return Textarea::make('reason')->required()->maxLength(2000);
    }

    /** @return array<TextInput> */
    private static function userFields(): array
    {
        return [
            TextInput::make('username')->required()->minLength(3)->maxLength(24),
            TextInput::make('name')->maxLength(255),
            TextInput::make('email')->email()->required()->maxLength(255),
            TextInput::make('password')->password()->required(),
            TextInput::make('password_confirmation')->password()->required(),
        ];
    }

    /** @return array<string, string> */
    private static function roleOptions(): array
    {
        return collect(UserRole::cases())->mapWithKeys(fn (UserRole $role): array => [$role->value => $role->getLabel()])->all();
    }

    private static function planSummary(?Plan $plan, bool $usesDefault): string
    {
        $plan ??= $usesDefault ? Plan::query()->default(config('filebeam.transfers.default_plan'))->active()->first() : null;

        if ($plan === null) {
            return $usesDefault ? 'Configured default plan (not found)' : 'No plan assigned';
        }

        return self::planDescription($plan, $usesDefault);
    }

    private static function planPreview(mixed $planId): string
    {
        $usesDefault = blank($planId);
        if ($usesDefault) {
            $plan = Plan::query()->default(config('filebeam.transfers.default_plan'))->active()->first();
        } else {
            $planId = filter_var($planId, FILTER_VALIDATE_INT, ['options' => ['min_range' => 1]]);
            $plan = $planId === false ? null : Plan::query()->whereKey($planId)->first();
        }

        if ($plan === null) {
            return $usesDefault ? 'Configured default plan could not be found.' : 'Select an active plan to preview its limits.';
        }

        return self::planDescription($plan, $usesDefault);
    }

    private static function planDescription(Plan $plan, bool $usesDefault): string
    {
        return sprintf(
            '%s%s: %s transfer, %s files, %s default file retention.',
            $plan->name,
            $usesDefault ? ' (configured default)' : '',
            PlanResource::formatBytes($plan->maximum_transfer_bytes),
            number_format($plan->maximum_file_count),
            PlanResource::formatHours($plan->default_file_retention_hours),
        );
    }

    private static function actor(): User
    {
        $actor = Filament::auth()->user();

        abort_unless($actor instanceof User, 403);

        return $actor;
    }
}
