<?php

declare(strict_types=1);

use Illuminate\Database\Connectors\ConnectionFactory;

test('sqlite immediate transactions reserve the writer before reads', function (): void {
    $database = tempnam(sys_get_temp_dir(), 'filebeam-sqlite-');

    if ($database === false) {
        throw new RuntimeException('Unable to create an isolated SQLite database.');
    }

    try {
        $config = array_replace(config('database.connections.sqlite'), [
            'database' => $database,
            'foreign_key_constraints' => true,
        ]);

        $factory = new ConnectionFactory($this->app);
        $first = $factory->make($config, 'sqlite-first');
        $second = $factory->make(array_replace($config, ['busy_timeout' => 100]), 'sqlite-second');

        $busyTimeout = config('database.connections.sqlite.busy_timeout');

        expect(config('database.connections.sqlite.transaction_mode'))->toBe('IMMEDIATE')
            ->and($busyTimeout)->toBeInt()
            ->and($first->scalar('pragma foreign_keys'))->toBe(1)
            ->and($first->scalar('pragma busy_timeout'))->toBe($busyTimeout)
            ->and($second->scalar('pragma foreign_keys'))->toBe(1)
            ->and($second->scalar('pragma busy_timeout'))->toBe(100);

        $first->statement('create table counters (value integer not null)');
        $first->statement('insert into counters (value) values (0)');

        $first->beginTransaction();

        expect($second->table('counters')->value('value'))->toBe(0);
        expect(fn () => $second->beginTransaction())
            ->toThrow(PDOException::class, 'database is locked');

        $first->table('counters')->increment('value');
        $first->commit();

        expect($second->table('counters')->value('value'))->toBe(1);

        $second->beginTransaction();
        $second->table('counters')->increment('value');
        $second->commit();

        expect($first->table('counters')->value('value'))->toBe(2);
    } finally {
        if (isset($first)) {
            $first->rollBack();
            $first->disconnect();
        }

        if (isset($second)) {
            $second->rollBack();
            $second->disconnect();
        }

        unlink($database);
    }
});
