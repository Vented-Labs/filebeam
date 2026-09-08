<?php

declare(strict_types=1);

return [

    'ssr' => [
        'enabled' => true,
        'url' => 'http://127.0.0.1:13714',

    ],

    'pages' => [

        'paths' => [
            resource_path('js/pages'),
        ],

        'extensions' => [
            'js',
            'jsx',
            'svelte',
            'ts',
            'tsx',
            'vue',
        ],

    ],

    'testing' => [

        'ensure_pages_exist' => true,

    ],

];
