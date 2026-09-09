@props(['name', 'size' => 20, 'label' => null])

{!! \App\Support\Icons\Iconsax::render($name, (int) $size, $label) !!}
