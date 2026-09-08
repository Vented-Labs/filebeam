@include('errors.page', [
    'status' => 500,
    'title' => 'Something went wrong',
    'description' => 'The service could not complete your request. Try again shortly.',
])
