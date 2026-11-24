use crate::ui::theme::{appearance, metrics as m, typography as ty};
use crate::{
    app,
    domain::{ChatMessage, DeleteScope, MessagePreview},
};
use iced::{
    Element, Fill, Point, Size,
    widget::{button, center, column, container, opaque, pin, responsive, row, text},
};

pub(in crate::ui) fn message_menu(
    menu: app::MessageMenu,
    can_reply: bool,
    can_forward: bool,
    can_change: bool,
) -> Element<'static, app::Message> {
    responsive(move |available| {
        let mut actions = column![
            menu_button(
                "Reply",
                can_reply.then_some(app::Message::SelectReply(menu.message_id))
            ),
            menu_button(
                "Forward",
                can_forward.then_some(app::Message::StartForward(menu.message_id))
            ),
        ]
        .spacing(m::MENU_SPACING);
        let mut item_count = 2;
        if menu.actions.can_edit {
            actions = actions.push(menu_button(
                "Edit",
                can_change.then_some(app::Message::StartEdit(menu.message_id)),
            ));
            item_count += 1;
        }
        if menu.actions.can_delete_for_self || menu.actions.can_delete_for_everyone {
            actions = actions.push(menu_button(
                "Delete",
                can_change.then_some(app::Message::ShowDeleteConfirmation(menu.message_id)),
            ));
            item_count += 1;
        }
        if menu.request_id.is_some() {
            actions = actions.push(
                container(text("Checking actions…").size(ty::CAPTION))
                    .height(m::MENU_ITEM_HEIGHT)
                    .center_y(m::MENU_ITEM_HEIGHT),
            );
            item_count += 1;
        }
        let height = m::MENU_PADDING * 2.0
            + m::MENU_ITEM_HEIGHT * item_count as f32
            + m::MENU_SPACING * (item_count - 1) as f32;
        let panel = container(actions)
            .width(m::MENU_WIDTH)
            .height(height)
            .padding(m::MENU_PADDING)
            .style(container::rounded_box);
        pin(opaque(panel))
            .position(menu_position(menu.position, available, height))
            .into()
    })
    .into()
}

fn menu_button(
    label: &'static str,
    action: Option<app::Message>,
) -> iced::widget::Button<'static, app::Message> {
    button(label)
        .width(Fill)
        .height(m::MENU_ITEM_HEIGHT)
        .on_press_maybe(action)
}

fn menu_position(cursor: Point, available: Size, height: f32) -> Point {
    Point::new(
        cursor
            .x
            .clamp(0.0, (available.width - m::MENU_WIDTH).max(0.0)),
        cursor.y.clamp(0.0, (available.height - height).max(0.0)),
    )
}

pub(in crate::ui) fn delete_confirmation<'a>(
    confirmation: &app::DeleteConfirmation,
    original: Option<&ChatMessage>,
    pending: bool,
    error: Option<&'a str>,
) -> Element<'a, app::Message> {
    let mut actions = row![
        button(if pending { "Close" } else { "Cancel" }).on_press(app::Message::CancelDelete),
    ]
    .spacing(m::GAP);
    if confirmation.actions.can_delete_for_self {
        actions = actions.push(
            button("Delete for me")
                .style(button::danger)
                .on_press_maybe(
                    (!pending).then_some(app::Message::ConfirmDelete(DeleteScope::ForSelf)),
                ),
        );
    }
    if confirmation.actions.can_delete_for_everyone {
        actions = actions.push(
            button("Delete for everyone")
                .style(button::danger)
                .on_press_maybe(
                    (!pending).then_some(app::Message::ConfirmDelete(DeleteScope::ForEveryone)),
                ),
        );
    }
    let mut dialog = column![
        text(if pending {
            "Deleting message…"
        } else {
            "Delete this message?"
        })
        .size(ty::DIALOG_TITLE),
    ]
    .spacing(m::GAP_SECTION);
    if let Some(original) = original {
        dialog = dialog.push(
            text(
                MessagePreview::message_snippet(&original.text, &original.text_runs).into_string(),
            )
            .size(ty::PREVIEW),
        );
    }
    dialog = dialog.push(actions);
    if let Some(error) = error {
        dialog = dialog.push(text(error).style(appearance::error_text));
    }
    center(opaque(
        container(dialog)
            .padding(m::DIALOG_PADDING)
            .style(container::rounded_box),
    ))
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_menu_stays_inside_the_window() {
        let window = Size::new(800.0, 600.0);
        assert_eq!(
            menu_position(Point::new(200.0, 300.0), window, 76.0),
            Point::new(200.0, 300.0)
        );
        assert_eq!(
            menu_position(Point::new(799.0, 599.0), window, 144.0),
            Point::new(648.0, 456.0)
        );
        assert_eq!(
            menu_position(Point::new(10.0, 10.0), Size::new(80.0, 50.0), 76.0),
            Point::ORIGIN
        );
    }
}
