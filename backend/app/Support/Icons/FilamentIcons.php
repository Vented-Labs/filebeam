<?php

declare(strict_types=1);

namespace App\Support\Icons;

use LogicException;

final class FilamentIcons
{
    /** @return array<string, string> */
    public static function aliases(): array
    {
        return array_map(static fn (string $icon): string => "filebeam-{$icon}", self::map([
            'menu' => ['actions::action-group', 'forms::components.builder.actions.reorder', 'forms::components.key-value.actions.reorder', 'forms::components.repeater.actions.reorder', 'schema::components.tabs.more-tabs-button', 'tables::actions.open-bulk-actions'],
            'plus' => ['actions::create-action.grouped', 'actions::import-action.grouped', 'forms::components.select.actions.create-option', 'panels::tenant-menu.registration-button', 'query-builder::add-rule-action', 'query-builder::or-group.add-group-action'],
            'trash' => ['actions::delete-action', 'actions::delete-action.grouped', 'actions::delete-action.modal', 'actions::force-delete-action', 'actions::force-delete-action.grouped', 'actions::force-delete-action.modal', 'forms::components.builder.actions.delete', 'forms::components.key-value.actions.delete', 'forms::components.repeater.actions.delete', 'forms::components.rich-editor.panels.custom-block.delete-button'],
            'x' => ['actions::detach-action', 'actions::detach-action.modal', 'actions::dissociate-action', 'actions::dissociate-action.modal', 'badge.delete-button', 'forms::components.rich-editor.panels.custom-blocks.close-button', 'forms::components.rich-editor.panels.merge-tags.close-button', 'forms::components.toggle-buttons.boolean.false', 'infolists::components.icon-entry.false', 'modal.close-button', 'notifications::database.modal.empty-state', 'notifications::notification.close-button', 'tables::columns.icon-column.false', 'tables::empty-state', 'tables::filters.remove-all-button', 'widgets::chart-widget.empty-state'],
            'download' => ['actions::export-action.grouped'],
            'alert' => ['actions::modal.confirmation', 'notifications::notification.danger', 'notifications::notification.warning', 'schema::components.callout.danger', 'schema::components.callout.warning'],
            'copy' => ['actions::replicate-action', 'actions::replicate-action.grouped', 'forms::components.builder.actions.clone', 'forms::components.repeater.actions.clone', 'forms::components.text-input.actions.copy'],
            'undo' => ['actions::restore-action', 'actions::restore-action.grouped', 'actions::restore-action.modal'],
            'updates' => ['panels::auth.multi-factor.app.actions.regenerate-recovery-codes', 'panels::auth.multi-factor.app.actions.regenerate-recovery-codes.modal', 'panels::auth.multi-factor.app.actions.regenerate-recovery-codes.notification'],
            'eye' => ['actions::view-action', 'actions::view-action.grouped', 'forms::components.text-input.actions.show-password', 'panels::resources.pages.view-record.navigation-item'],
            'eye-off' => ['forms::components.text-input.actions.hide-password'],
            'info' => ['notifications::notification.info', 'schema::components.callout.info'],
            'check' => ['forms::components.toggle-buttons.boolean.true', 'infolists::components.icon-entry.true', 'notifications::notification.success', 'query-builder::constraints.boolean', 'schema::components.callout.success', 'schema::components.wizard.completed-step', 'tables::actions.disable-reordering', 'tables::columns.icon-column.true'],
            'arrow-right' => ['breadcrumbs.separator', 'pagination.first-button.rtl', 'pagination.last-button', 'pagination.next-button', 'pagination.previous-button.rtl', 'panels::pages.password-reset.request-password-reset.actions.login.rtl', 'panels::sidebar.collapse-button.rtl', 'panels::sidebar.expand-button', 'panels::topbar.open-sidebar-button'],
            'arrow-left' => ['breadcrumbs.separator.rtl', 'pagination.first-button', 'pagination.last-button.rtl', 'pagination.next-button.rtl', 'pagination.previous-button', 'panels::pages.password-reset.request-password-reset.actions.login', 'panels::sidebar.collapse-button', 'panels::sidebar.expand-button.rtl', 'panels::topbar.close-sidebar-button'],
            'arrow-down' => ['forms::components.builder.actions.move-down', 'forms::components.repeater.actions.move-down'],
            'arrow-up' => ['forms::components.builder.actions.move-up', 'forms::components.repeater.actions.move-up'],
            'chevron-down' => ['forms::components.builder.actions.collapse', 'forms::components.repeater.actions.collapse', 'panels::sidebar.group.collapse-button', 'panels::sub-navigation.mobile-menu.button', 'panels::tenant-menu.toggle-button', 'panels::topbar.group.toggle-button', 'panels::user-menu.toggle-button', 'schema::components.tabs.dropdown-trigger-button', 'section.collapse-button', 'tables::columns.collapse-button', 'tables::grouping.collapse-button'],
            'chevron-up' => ['forms::components.builder.actions.expand', 'forms::components.repeater.actions.expand'],
            'edit' => ['actions::edit-action', 'actions::edit-action.grouped', 'forms::components.modal-table-select.actions.select', 'forms::components.rich-editor.panels.custom-block.edit-button', 'forms::components.select.actions.edit-option', 'panels::resources.pages.edit-record.navigation-item'],
            'search' => ['forms::components.checkbox-list.search-field', 'panels::global-search.field', 'tables::search-field'],
            'crop' => ['forms::components.file-upload.editor.actions.drag-crop'],
            'move' => ['forms::components.file-upload.editor.actions.drag-move', 'forms::components.file-upload.editor.actions.move-down', 'forms::components.file-upload.editor.actions.move-left', 'forms::components.file-upload.editor.actions.move-right', 'forms::components.file-upload.editor.actions.move-up', 'tables::actions.enable-reordering', 'tables::reorder.handle'],
            'flip' => ['forms::components.file-upload.editor.actions.flip-horizontal', 'forms::components.file-upload.editor.actions.flip-vertical'],
            'rotate-left' => ['forms::components.file-upload.editor.actions.rotate-left'],
            'rotate-right' => ['forms::components.file-upload.editor.actions.rotate-right'],
            'maximize' => ['forms::components.file-upload.editor.actions.zoom-100'],
            'zoom-in' => ['forms::components.file-upload.editor.actions.zoom-in'],
            'zoom-out' => ['forms::components.file-upload.editor.actions.zoom-out'],
            'settings' => ['forms::components.builder.actions.edit', 'panels::tenant-menu.profile-button'],
            'filter' => ['panels::pages.dashboard.actions.filter', 'tables::actions.filter', 'widgets::chart-widget.filter'],
            'credit-card' => ['panels::tenant-menu.billing-button'],
            'bell' => ['panels::topbar.open-database-notifications-button', 'panels::sidebar.open-database-notifications-button'],
            'number' => ['query-builder::constraints.number'],
            'relationship' => ['query-builder::constraints.relationship', 'tables::actions.group'],
            'select' => ['query-builder::constraints.select'],
            'text' => ['query-builder::constraints.text'],
            'slash' => ['query-builder::or-group.block'],
            'columns' => ['tables::actions.column-manager'],
            'sort' => ['tables::header-cell.sort-asc-button', 'tables::header-cell.sort-button', 'tables::header-cell.sort-desc-button'],
            'sun' => ['panels::theme-switcher.light-button'],
            'moon' => ['panels::theme-switcher.dark-button'],
            'monitor' => ['panels::theme-switcher.system-button'],
            'lock' => ['panels::auth.multi-factor.app.actions.set-up', 'panels::auth.multi-factor.app.actions.set-up.modal', 'panels::auth.multi-factor.app.actions.set-up.notification', 'panels::auth.multi-factor.email.actions.set-up', 'panels::auth.multi-factor.email.actions.set-up.modal', 'panels::auth.multi-factor.email.actions.set-up.notification'],
            'unlock' => ['panels::auth.multi-factor.app.actions.disable', 'panels::auth.multi-factor.app.actions.disable.modal', 'panels::auth.multi-factor.app.actions.disable.notification', 'panels::auth.multi-factor.email.actions.disable', 'panels::auth.multi-factor.email.actions.disable.modal', 'panels::auth.multi-factor.email.actions.disable.notification'],
            'dashboard' => ['panels::pages.dashboard.navigation-item'],
            'folder' => ['panels::resources.pages.manage-related-records.navigation-item'],
            'calendar' => ['query-builder::constraints.date'],
            'user' => ['panels::user-menu.profile-item'],
            'logout' => ['panels::user-menu.logout-button', 'panels::widgets.account.logout-button'],
            'book' => ['panels::widgets.filament-info.open-documentation-button'],
            'code' => ['panels::widgets.filament-info.open-github-button'],
        ]));
    }

    public static function replaceDirectReferences(string $source): string
    {
        return preg_replace_callback('/(?:\\\\?Filament\\\\Support\\\\Icons\\\\)?Heroicon::([A-Za-z0-9_]+)/', static function (array $match): string {
            $icon = self::directReferences()[$match[1]] ?? null;

            if ($icon === null) {
                throw new LogicException("Unmapped Filament built-in icon: {$match[1]}");
            }

            return "'filebeam-{$icon}'";
        }, $source) ?? $source;
    }

    /** @return array<string, string> */
    public static function directReferences(): array
    {
        return self::map([
            'arrow-down' => ['ArrowDown', 'ChevronDown'],
            'arrow-down-circle' => ['ArrowDownCircle'],
            'download' => ['ArrowDownTray'],
            'arrow-left' => ['ArrowLeft', 'ChevronDoubleLeft', 'ChevronLeft', 'OutlinedChevronLeft'],
            'arrow-left-circle' => ['ArrowLeftCircle'],
            'logout' => ['ArrowLeftEndOnRectangle'],
            'updates' => ['ArrowPath', 'OutlinedArrowPath'],
            'arrow-right' => ['ArrowRight', 'ChevronDoubleRight', 'ChevronRight', 'OutlinedChevronRight'],
            'arrow-right-circle' => ['ArrowRightCircle'],
            'arrow-up' => ['ArrowUp', 'ChevronUp'],
            'arrow-up-circle' => ['ArrowUpCircle'],
            'upload' => ['ArrowUpTray'],
            'undo' => ['ArrowUturnLeft', 'OutlinedArrowUturnLeft'],
            'redo' => ['ArrowUturnRight'],
            'select' => ['ChevronUpDown'],
            'menu' => ['Bars2', 'EllipsisHorizontal', 'EllipsisVertical', 'OutlinedBars3'],
            'bold' => ['Bold'],
            'italic' => ['Italic'],
            'list' => ['ListBullet'],
            'ordered-list' => ['NumberedList'],
            'strikethrough' => ['Strikethrough'],
            'underline' => ['Underline'],
            'check' => ['Check', 'CheckCircle', 'OutlinedCheck', 'OutlinedCheckCircle'],
            'x' => ['Minus', 'OutlinedXCircle', 'OutlinedXMark', 'XMark'],
            'eye' => ['Eye', 'OutlinedEye'],
            'eye-off' => ['EyeSlash'],
            'alert' => ['OutlinedExclamationCircle', 'OutlinedExclamationTriangle'],
            'lock' => ['LockClosed', 'LockOpen', 'OutlinedLockClosed', 'OutlinedLockOpen'],
            'settings' => ['AdjustmentsHorizontal', 'AdjustmentsVertical', 'Cog6Tooth'],
            'maximize' => ['ArrowsPointingOut'],
            'move' => ['ArrowsUpDown'],
            'calendar' => ['Calendar'],
            'monitor' => ['ComputerDesktop'],
            'credit-card' => ['CreditCard'],
            'filter' => ['Funnel'],
            'language' => ['Language'],
            'link' => ['Link'],
            'search' => ['MagnifyingGlass'],
            'zoom-out' => ['MagnifyingGlassMinus'],
            'zoom-in' => ['MagnifyingGlassPlus'],
            'attachment' => ['PaperClip'],
            'edit' => ['OutlinedPencilSquare', 'PencilSquare'],
            'slash' => ['Slash'],
            'copy' => ['ClipboardDocumentList', 'Square2Stack'],
            'table' => ['SquaresPlus'],
            'color' => ['Swatch'],
            'number' => ['Variable'],
            'columns' => ['ViewColumns'],
            'archive' => ['ArchiveBox'],
            'book' => ['BookOpen'],
            'home' => ['Home', 'OutlinedHome'],
            'folder' => ['OutlinedRectangleStack', 'OutlinedSquares2x2', 'RectangleStack'],
            'quote' => ['ChatBubbleBottomCenterText'],
            'bell' => ['OutlinedBell'],
            'bell-off' => ['OutlinedBellSlash'],
            'info' => ['OutlinedInformationCircle'],
            'user' => ['UserCircle'],
            'trash' => ['OutlinedTrash', 'Trash'],
            'plus' => ['Plus'],
            'moon' => ['Moon'],
            'sun' => ['Sun'],
        ]);
    }

    /** @param array<string, list<string>> $icons
     * @return array<string, string>
     */
    private static function map(array $icons): array
    {
        $map = [];

        foreach ($icons as $icon => $names) {
            foreach ($names as $name) {
                if (isset($map[$name])) {
                    throw new LogicException("Duplicate Filament icon mapping: {$name}");
                }

                $map[$name] = $icon;
            }
        }

        return $map;
    }
}
