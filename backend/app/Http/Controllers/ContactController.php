<?php

declare(strict_types=1);

namespace App\Http\Controllers;

use App\Enums\ReceivingPolicy;
use App\Models\ContactBlock;
use App\Models\ContactPreference;
use App\Models\Friendship;
use App\Models\User;
use App\Notifications\FriendshipChanged;
use App\Support\ReceivingPermissions;
use Illuminate\Http\JsonResponse;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\DB;
use Illuminate\Validation\Rule;
use Inertia\Inertia;
use Inertia\Response;

class ContactController extends Controller
{
    public function page(): Response
    {
        return Inertia::render('Contacts');
    }

    public function index(Request $request): JsonResponse
    {
        $owner = $request->user();
        assert($owner instanceof User);
        $owner->refresh();
        $friendships = Friendship::query()->involving($owner->id)->with('preferences')->orderBy('id')->get();
        $blocks = ContactBlock::query()->where('user_id', $owner->id)->get();
        $ids = $friendships->flatMap(fn (Friendship $friendship): array => [$friendship->lower_user_id, $friendship->upper_user_id])->merge($blocks->pluck('blocked_user_id'));
        $users = User::query()->whereKey($ids)->get()->keyBy('id');
        $contacts = $friendships->map(function (Friendship $friendship) use ($owner, $users): array {
            $other = $users->get($friendship->lower_user_id === $owner->id ? $friendship->upper_user_id : $friendship->lower_user_id);
            assert($other instanceof User);
            $preference = $friendship->preferences->firstWhere('user_id', $owner->id);

            return [
                ...$this->identity($other),
                'status' => $friendship->accepted_at !== null ? 'accepted' : ($friendship->requester_id === $owner->id ? 'outgoing' : 'incoming'),
                'canSend' => $preference?->can_send,
                'autoDownload' => $preference?->auto_download,
                'effective' => app(ReceivingPermissions::class)->resolve($owner, $other, true),
            ];
        });
        $blocked = $blocks->map(function (ContactBlock $block) use ($users): array {
            $other = $users->get($block->blocked_user_id);
            assert($other instanceof User);

            return $this->identity($other);
        });

        return $this->json(['settings' => $this->settings($owner), 'contacts' => $contacts->all(), 'blocked' => $blocked->all()]);
    }

    public function lookup(Request $request, string $username): JsonResponse
    {
        $owner = $request->user();
        assert($owner instanceof User);
        $other = $this->target($username);
        abort_if($other->id === $owner->id || ContactBlock::query()->between($owner->id, $other->id)->exists(), 404);

        return $this->json($this->identity($other));
    }

    public function defaults(Request $request): JsonResponse
    {
        $data = $request->validate([
            'receivingPolicy' => ['required', Rule::enum(ReceivingPolicy::class)],
            'autoDownloadFriends' => ['required', 'boolean'],
        ]);
        $owner = DB::transaction(function () use ($request, $data): User {
            $owner = User::query()->lockForUpdate()->findOrFail($request->user()->id);
            $owner->update(['receiving_policy' => $data['receivingPolicy'], 'auto_download_friends' => $data['autoDownloadFriends']]);
            $owner->increment('receiving_revision');

            return $owner;
        }, 3);

        return $this->json($this->settings($owner));
    }

    public function mutate(Request $request, string $username): JsonResponse
    {
        $data = $request->validate([
            'action' => ['required', Rule::in(['request', 'accept', 'decline', 'cancel', 'remove', 'block', 'unblock', 'preferences'])],
            'canSend' => ['present_if:action,preferences', 'nullable', 'boolean'],
            'autoDownload' => ['present_if:action,preferences', 'nullable', 'boolean'],
        ]);
        $target = $this->target($username, in_array($data['action'], ['request', 'accept'], true));
        $ownerId = $request->user()->id;
        abort_if($target->id === $ownerId, 422, 'You cannot add yourself.');
        DB::transaction(function () use ($ownerId, $target, $data): void {
            // Stable account locks serialize mutations even before a pair exists.
            $users = User::query()->whereKey([$ownerId, $target->id])->orderBy('id')->lockForUpdate()->get()->keyBy('id');
            $owner = $users->get($ownerId);
            $other = $users->get($target->id);
            abort_unless($owner instanceof User && $other instanceof User && $owner->suspended_at === null, 404);
            abort_if(in_array($data['action'], ['request', 'accept'], true) && $other->suspended_at !== null, 404);
            $friendship = Friendship::query()->between($ownerId, $other->id)->first();
            $action = $data['action'];
            if ($action === 'block') {
                ContactBlock::query()->firstOrCreate(['user_id' => $ownerId, 'blocked_user_id' => $other->id]);
                $friendship?->delete();
            } elseif ($action === 'unblock') {
                ContactBlock::query()->where('user_id', $ownerId)->where('blocked_user_id', $other->id)->delete();
            } else {
                abort_if(ContactBlock::query()->between($ownerId, $other->id)->exists(), 404);
                if ($action === 'request') {
                    if ($friendship === null) {
                        Friendship::query()->create(['lower_user_id' => min($ownerId, $other->id), 'upper_user_id' => max($ownerId, $other->id), 'requester_id' => $ownerId]);
                        $other->notify(new FriendshipChanged(false));
                    }
                } elseif ($action === 'accept') {
                    abort_unless($friendship !== null && $friendship->requester_id !== $ownerId, 409);
                    if ($friendship->accepted_at === null) {
                        $friendship->update(['accepted_at' => now()]);
                        $other->notify(new FriendshipChanged(true));
                    }
                } elseif ($action === 'preferences') {
                    abort_unless($friendship !== null && $friendship->accepted_at !== null, 409);
                    ContactPreference::query()->updateOrCreate(['friendship_id' => $friendship->id, 'user_id' => $ownerId], ['can_send' => $data['canSend'], 'auto_download' => $data['autoDownload']]);
                } elseif ($friendship !== null) {
                    abort_unless(match ($action) {
                        'cancel' => $friendship->accepted_at === null && $friendship->requester_id === $ownerId,
                        'decline' => $friendship->accepted_at === null && $friendship->requester_id !== $ownerId,
                        'remove' => $friendship->accepted_at !== null,
                        default => false,
                    }, 409);
                    $friendship->delete();
                }
            }
            $owner->increment('receiving_revision');
            $other->increment('receiving_revision');
        }, 3);

        return $this->index($request);
    }

    private function target(string $username, bool $active = true): User
    {
        return User::query()->where('normalized_username', strtolower($username))->when($active, fn ($query) => $query->whereNull('suspended_at'))->firstOrFail();
    }

    /** @return array{id: int, username: string, name: string} */
    private function identity(User $user): array
    {
        return ['id' => $user->id, 'username' => $user->username, 'name' => $user->name];
    }

    /** @return array{receivingPolicy: string, autoDownloadFriends: bool, revision: int} */
    private function settings(User $user): array
    {
        return ['receivingPolicy' => $user->receiving_policy->value, 'autoDownloadFriends' => $user->auto_download_friends, 'revision' => $user->receiving_revision];
    }

    /** @param array<string, mixed> $data */
    private function json(array $data): JsonResponse
    {
        return response()->json(['data' => $data], 200, ['Cache-Control' => 'no-store, private']);
    }
}
