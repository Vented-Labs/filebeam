<?php

declare(strict_types=1);

namespace App\Http\Controllers;

use App\Models\User;
use App\Support\InstanceSettings;
use Illuminate\Http\Request;
use Inertia\Inertia;
use Inertia\Response;

class ReceiveController extends Controller
{
    public function __invoke(Request $request, string $username): Response
    {
        abort_unless(app(InstanceSettings::class)->boolean('username_routing'), 404);

        $recipient = User::query()
            ->where('normalized_username', strtolower($username))
            ->inboxEnabled()
            ->whereHas('activeAccountKeyBundles')
            ->with('activeAccountKeyBundles')
            ->firstOrFail();
        $bundle = $recipient->activeAccountKeyBundles->sole();

        return Inertia::render('Receive', ['recipient' => [
            'id' => $recipient->id,
            'username' => $recipient->username,
            'public_key' => $bundle->public_key,
            'account_key_bundle_id' => $bundle->id,
            'version' => $bundle->version,
            'fingerprint' => $bundle->fingerprint,
        ]]);
    }
}
