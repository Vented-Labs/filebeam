@include('errors.page', [
    'status' => 403,
    'title' => 'You do not have access',
    'description' => 'This area is not available to your account. If you think this is a mistake, sign in with the correct account and try again.',
])
