#!/usr/bin/env php
<?php

declare(strict_types=1);

use Filebeam\Updater\Command;

require __DIR__.'/updater/Updater.php';

exit(Command::run($argv, __DIR__));
