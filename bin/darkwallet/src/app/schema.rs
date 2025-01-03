/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use sled_overlay::sled;

use crate::{
    error::Error,
    expr::{self, Compiler, Op},
    gfx::{GraphicsEventPublisherPtr, Rectangle, RenderApi, Vertex},
    mesh::{Color, MeshBuilder},
    prop::{
        Property, PropertyBool, PropertyFloat32, PropertyStr, PropertySubType, PropertyType, Role,
    },
    scene::{SceneNodePtr, Slot},
    text::TextShaperPtr,
    ui::{
        Button, ChatEdit, ChatView, EditBox, Image, Layer, ShapeVertex, Text, VectorArt,
        VectorShape, Window,
    },
    ExecutorPtr,
};

use super::{
    node::{
        create_button, create_chatedit, create_chatview, create_editbox, create_image,
        create_layer, create_text, create_vector_art,
    },
    populate_tree, App,
};

const LIGHTMODE: bool = false;

mod android_ui_consts {
    pub const EDITCHAT_HEIGHT: f32 = 163.;
    pub const EDITCHAT_BOTTOM_PAD: f32 = 30.;
    pub const EDITCHAT_CURSOR_ASCENT: f32 = 50.;
    pub const EDITCHAT_CURSOR_DESCENT: f32 = 20.;
    pub const EDITCHAT_SELECT_ASCENT: f32 = 50.;
    pub const EDITCHAT_SELECT_DESCENT: f32 = 20.;
    pub const EDITCHAT_HANDLE_DESCENT: f32 = 10.;
    pub const EDITCHAT_LINESPACING: f32 = 70.;
    pub const TEXTBAR_BASELINE: f32 = 60.;
    pub const TEXT_DESCENT: f32 = 20.;
    pub const EDITCHAT_LHS_PAD: f32 = 30.;
    pub const SENDLABEL_WIDTH: f32 = 200.;
    pub const SENDLABEL_LHS_PAD: f32 = 40.;
    pub const FONTSIZE: f32 = 40.;
    pub const TIMESTAMP_FONTSIZE: f32 = 30.;
    pub const TIMESTAMP_WIDTH: f32 = 135.;
    pub const MESSAGE_SPACING: f32 = 15.;
    pub const LINE_HEIGHT: f32 = 58.;
    pub const CHATVIEW_BASELINE: f32 = 36.;
}

#[cfg(target_os = "android")]
mod ui_consts {
    pub const CHATDB_PATH: &str = "/data/data/darkfi.darkwallet/chatdb/";
    pub const KING_PATH: &str = "king.png";
    pub const BG_PATH: &str = "bg.png";
    pub use super::android_ui_consts::*;
}

#[cfg(feature = "emulate-android")]
mod ui_consts {
    pub const CHATDB_PATH: &str = "chatdb";
    pub const KING_PATH: &str = "assets/king.png";
    pub const BG_PATH: &str = "assets/bg.png";
    pub use super::android_ui_consts::*;
}

#[cfg(all(
    any(target_os = "linux", target_os = "macos", target_os = "windows"),
    not(feature = "emulate-android")
))]
mod ui_consts {
    pub const CHATDB_PATH: &str = "chatdb";
    pub const KING_PATH: &str = "assets/king.png";
    pub const BG_PATH: &str = "assets/bg.png";
    pub const EDITCHAT_HEIGHT: f32 = 50.;
    pub const EDITCHAT_BOTTOM_PAD: f32 = 6.;
    pub const EDITCHAT_CURSOR_ASCENT: f32 = 25.;
    pub const EDITCHAT_CURSOR_DESCENT: f32 = 8.;
    pub const EDITCHAT_SELECT_ASCENT: f32 = 30.;
    pub const EDITCHAT_SELECT_DESCENT: f32 = 10.;
    pub const EDITCHAT_HANDLE_DESCENT: f32 = 35.;
    pub const EDITCHAT_LINESPACING: f32 = 35.;
    pub const TEXTBAR_BASELINE: f32 = 34.;
    pub const TEXT_DESCENT: f32 = 10.;
    pub const EDITCHAT_LHS_PAD: f32 = 20.;
    pub const SENDLABEL_WIDTH: f32 = 120.;
    pub const SENDLABEL_LHS_PAD: f32 = 30.;
    pub const FONTSIZE: f32 = 20.;
    pub const TIMESTAMP_FONTSIZE: f32 = 12.;
    pub const TIMESTAMP_WIDTH: f32 = 60.;
    pub const MESSAGE_SPACING: f32 = 5.;
    pub const LINE_HEIGHT: f32 = 30.;
    pub const CHATVIEW_BASELINE: f32 = 20.;
}

use ui_consts::*;

pub(super) async fn make(app: &App, window: SceneNodePtr) {
    let window_scale = PropertyFloat32::wrap(&window, Role::Internal, "scale", 0).unwrap();

    let mut cc = Compiler::new();

    cc.add_const_f32("EDITCHAT_HEIGHT", EDITCHAT_HEIGHT);
    cc.add_const_f32("EDITCHAT_BOTTOM_PAD", EDITCHAT_BOTTOM_PAD);
    cc.add_const_f32("SENDLABEL_WIDTH", SENDLABEL_WIDTH);
    cc.add_const_f32("SENDLABEL_LHS_PAD", SENDLABEL_LHS_PAD);

    // Main view
    let layer_node = create_layer("view");
    let prop = layer_node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, 0.).unwrap();
    prop.set_f32(Role::App, 1, 0.).unwrap();
    prop.set_expr(Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(Role::App, 3, expr::load_var("h")).unwrap();
    layer_node.set_property_bool(Role::App, "is_visible", true).unwrap();
    let layer_node =
        layer_node.setup(|me| Layer::new(me, app.render_api.clone(), app.ex.clone())).await;
    window.link(layer_node.clone());

    // Create a bg image
    let node = create_image("bg_image");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, 0.).unwrap();
    prop.set_f32(Role::App, 1, 0.).unwrap();
    prop.set_expr(Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(Role::App, 3, expr::load_var("h")).unwrap();

    // Image aspect ratio
    //let R = 1.78;
    let R = 1.555;
    cc.add_const_f32("R", R);

    let prop = node.get_property("uv").unwrap();
    prop.set_f32(Role::App, 0, 0.).unwrap();
    prop.set_f32(Role::App, 1, 0.).unwrap();
    #[rustfmt::skip]
    let code = cc.compile("
        r = w / h;
        if r < R {
            r / R
        } else {
            1
        }
    ").unwrap();
    prop.set_expr(Role::App, 2, code).unwrap();
    #[rustfmt::skip]
    let code = cc.compile("
        r = w / h;
        if r < R {
            1
        } else {
            R / r
        }
    ").unwrap();
    prop.set_expr(Role::App, 3, code).unwrap();

    node.set_property_str(Role::App, "path", BG_PATH).unwrap();
    node.set_property_u32(Role::App, "z_index", 0).unwrap();
    let node = node.setup(|me| Image::new(me, app.render_api.clone(), app.ex.clone())).await;
    layer_node.clone().link(node);

    // Create a bg mesh
    let node = create_vector_art("bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, 0.).unwrap();
    prop.set_f32(Role::App, 1, 0.).unwrap();
    prop.set_expr(Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(Role::App, 3, expr::load_var("h")).unwrap();
    node.set_property_u32(Role::App, "z_index", 1).unwrap();

    let c = if LIGHTMODE { 1. } else { 0. };
    // Setup the pimpl
    let node_id = node.id;
    let mut shape = VectorShape::new();
    shape.add_filled_box(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::load_var("h"),
        [c, c, c, 0.3],
    );
    let node =
        node.setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone())).await;
    layer_node.clone().link(node);

    // Create the toolbar bg
    let node = create_vector_art("toolbar_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, 0.).unwrap();
    prop.set_f32(Role::App, 1, 0.).unwrap();
    prop.set_expr(Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_f32(Role::App, 3, EDITCHAT_HEIGHT).unwrap();
    node.set_property_u32(Role::App, "z_index", 2).unwrap();

    let mut shape = VectorShape::new();
    shape.add_filled_box(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::load_var("h"),
        [0.05, 0.05, 0.05, 1.],
    );
    shape.add_filled_box(
        expr::const_f32(0.),
        cc.compile("h - 1").unwrap(),
        expr::load_var("w"),
        expr::load_var("h"),
        [0.4, 0.4, 0.4, 1.],
    );

    let node =
        node.setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone())).await;
    layer_node.clone().link(node);

    // Create some text
    let node = create_text("channel_label");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, SENDLABEL_LHS_PAD).unwrap();
    prop.set_f32(Role::App, 1, 0.).unwrap();
    prop.set_expr(Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_f32(Role::App, 3, EDITCHAT_HEIGHT).unwrap();
    node.set_property_u32(Role::App, "z_index", 2).unwrap();
    node.set_property_f32(Role::App, "baseline", TEXTBAR_BASELINE).unwrap();
    node.set_property_f32(Role::App, "font_size", FONTSIZE).unwrap();
    node.set_property_str(Role::App, "text", "random").unwrap();
    //node.set_property_str(Role::App, "text", "anon1").unwrap();
    let prop = node.get_property("text_color").unwrap();
    prop.set_f32(Role::App, 0, 1.).unwrap();
    prop.set_f32(Role::App, 1, 1.).unwrap();
    prop.set_f32(Role::App, 2, 1.).unwrap();
    prop.set_f32(Role::App, 3, 1.).unwrap();
    node.set_property_u32(Role::App, "z_index", 3).unwrap();

    let node = node
        .setup(|me| {
            Text::new(
                me,
                window_scale.clone(),
                app.render_api.clone(),
                app.text_shaper.clone(),
                app.ex.clone(),
            )
        })
        .await;
    layer_node.clone().link(node);

    // ChatView
    let node = create_chatview("chatty");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, 10.).unwrap();
    prop.set_f32(Role::App, 1, EDITCHAT_HEIGHT).unwrap();
    let code = cc.compile("w - 30").unwrap();
    prop.set_expr(Role::App, 2, code).unwrap();
    let code = cc.compile("h - EDITCHAT_HEIGHT - editz_h - EDITCHAT_BOTTOM_PAD").unwrap();
    prop.set_expr(Role::App, 3, code).unwrap();
    let chatview_rect_prop = prop.clone();
    node.set_property_f32(Role::App, "font_size", FONTSIZE).unwrap();
    node.set_property_f32(Role::App, "timestamp_font_size", TIMESTAMP_FONTSIZE).unwrap();
    node.set_property_f32(Role::App, "timestamp_width", TIMESTAMP_WIDTH).unwrap();
    node.set_property_f32(Role::App, "line_height", LINE_HEIGHT).unwrap();
    node.set_property_f32(Role::App, "message_spacing", MESSAGE_SPACING).unwrap();
    node.set_property_f32(Role::App, "baseline", CHATVIEW_BASELINE).unwrap();
    node.set_property_u32(Role::App, "z_index", 2).unwrap();
    //node.set_property_bool(Role::App, "debug", true).unwrap();

    #[cfg(target_os = "android")]
    node.set_property_f32(Role::App, "scroll_start_accel", 40.).unwrap();
    #[cfg(target_os = "linux")]
    node.set_property_f32(Role::App, "scroll_start_accel", 15.).unwrap();

    node.set_property_f32(Role::App, "scroll_resist", 0.9).unwrap();

    let prop = node.get_property("timestamp_color").unwrap();
    prop.set_f32(Role::App, 0, 0.407).unwrap();
    prop.set_f32(Role::App, 1, 0.604).unwrap();
    prop.set_f32(Role::App, 2, 0.647).unwrap();
    prop.set_f32(Role::App, 3, 1.).unwrap();
    let prop = node.get_property("text_color").unwrap();
    if LIGHTMODE {
        prop.set_f32(Role::App, 0, 0.).unwrap();
        prop.set_f32(Role::App, 1, 0.).unwrap();
        prop.set_f32(Role::App, 2, 0.).unwrap();
        prop.set_f32(Role::App, 3, 1.).unwrap();
    } else {
        prop.set_f32(Role::App, 0, 1.).unwrap();
        prop.set_f32(Role::App, 1, 1.).unwrap();
        prop.set_f32(Role::App, 2, 1.).unwrap();
        prop.set_f32(Role::App, 3, 1.).unwrap();
    }

    let prop = node.get_property("nick_colors").unwrap();
    #[rustfmt::skip]
    let nick_colors = [
        0.00, 0.94, 1.00, 1.,
        0.36, 1.00, 0.69, 1.,
        0.29, 1.00, 0.45, 1.,
        0.00, 0.73, 0.38, 1.,
        0.21, 0.67, 0.67, 1.,
        0.56, 0.61, 1.00, 1.,
        0.84, 0.48, 1.00, 1.,
        1.00, 0.61, 0.94, 1.,
        1.00, 0.36, 0.48, 1.,
        1.00, 0.30, 0.00, 1.
    ];
    for c in nick_colors {
        prop.push_f32(Role::App, c).unwrap();
    }

    let prop = node.get_property("hi_bg_color").unwrap();
    if LIGHTMODE {
        prop.set_f32(Role::App, 0, 0.5).unwrap();
        prop.set_f32(Role::App, 1, 0.5).unwrap();
        prop.set_f32(Role::App, 2, 0.5).unwrap();
        prop.set_f32(Role::App, 3, 1.).unwrap();
    } else {
        prop.set_f32(Role::App, 0, 0.5).unwrap();
        prop.set_f32(Role::App, 1, 0.5).unwrap();
        prop.set_f32(Role::App, 2, 0.5).unwrap();
        prop.set_f32(Role::App, 3, 1.).unwrap();
    }

    let db = sled::open(CHATDB_PATH).expect("cannot open sleddb");
    let chat_tree = db.open_tree(b"chat").unwrap();
    //if chat_tree.is_empty() {
    //    populate_tree(&chat_tree);
    //}
    debug!(target: "app", "db has {} lines", chat_tree.len());
    let node = node
        .setup(|me| {
            ChatView::new(
                me,
                chat_tree,
                window_scale.clone(),
                app.render_api.clone(),
                app.text_shaper.clone(),
                app.ex.clone(),
            )
        })
        .await;
    layer_node.clone().link(node);

    // Create the editbox bg
    let node = create_vector_art("editbox_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, 0.).unwrap();
    let code = cc.compile("h - editz_h - 2 * EDITCHAT_BOTTOM_PAD").unwrap();
    prop.set_expr(Role::App, 1, code).unwrap();
    prop.set_expr(Role::App, 2, expr::load_var("w")).unwrap();
    let code = cc.compile("editz_h + 2 * EDITCHAT_BOTTOM_PAD").unwrap();
    prop.set_expr(Role::App, 3, code).unwrap();
    node.set_property_u32(Role::App, "z_index", 2).unwrap();

    let editbox_bg_rect_prop = prop.clone();

    let mut shape = VectorShape::new();
    shape.add_filled_box(
        expr::const_f32(0.),
        expr::const_f32(0.),
        cc.compile("w - SENDLABEL_WIDTH").unwrap(),
        expr::load_var("h"),
        [0., 0.13, 0.08, 1.],
    );
    shape.add_filled_box(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::const_f32(1.),
        [0.41, 0.6, 0.65, 1.],
    );
    shape.add_filled_box(
        cc.compile("w - SENDLABEL_WIDTH").unwrap(),
        expr::const_f32(0.),
        cc.compile("w - SENDLABEL_WIDTH - 1").unwrap(),
        expr::load_var("h"),
        [0.41, 0.6, 0.65, 1.],
    );
    shape.add_filled_box(
        cc.compile("w - SENDLABEL_WIDTH + 1").unwrap(),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::load_var("h"),
        [0.04, 0.04, 0.04, 1.],
    );
    let node =
        node.setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone())).await;
    layer_node.clone().link(node);

    // Create some text
    let node = create_text("send_label");
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("w - (SENDLABEL_WIDTH - SENDLABEL_LHS_PAD)").unwrap();
    prop.set_expr(Role::App, 0, code).unwrap();
    let code = cc.compile("h - EDITCHAT_HEIGHT").unwrap();
    prop.set_expr(Role::App, 1, code).unwrap();
    prop.set_f32(Role::App, 2, SENDLABEL_WIDTH).unwrap();
    prop.set_f32(Role::App, 3, EDITCHAT_HEIGHT).unwrap();
    node.set_property_f32(Role::App, "baseline", TEXTBAR_BASELINE).unwrap();
    node.set_property_f32(Role::App, "font_size", FONTSIZE).unwrap();
    node.set_property_str(Role::App, "text", "send").unwrap();
    //node.set_property_str(Role::App, "text", "anon1").unwrap();
    let prop = node.get_property("text_color").unwrap();
    prop.set_f32(Role::App, 0, 0.).unwrap();
    prop.set_f32(Role::App, 1, 1.).unwrap();
    prop.set_f32(Role::App, 2, 0.94).unwrap();
    prop.set_f32(Role::App, 3, 1.).unwrap();
    node.set_property_u32(Role::App, "z_index", 3).unwrap();

    let node = node
        .setup(|me| {
            Text::new(
                me,
                window_scale.clone(),
                app.render_api.clone(),
                app.text_shaper.clone(),
                app.ex.clone(),
            )
        })
        .await;
    layer_node.clone().link(node);

    // Text edit
    let node = create_chatedit("editz");
    node.set_property_bool(Role::App, "is_active", true).unwrap();
    node.set_property_bool(Role::App, "is_focused", true).unwrap();

    node.set_property_f32(Role::App, "max_height", 300.).unwrap();

    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, EDITCHAT_LHS_PAD).unwrap();
    let code = cc.compile("parent_h - rect_h - EDITCHAT_BOTTOM_PAD").unwrap();
    prop.set_expr(Role::App, 1, code).unwrap();
    let code = cc.compile("parent_w - (SENDLABEL_WIDTH + SENDLABEL_LHS_PAD)").unwrap();
    prop.set_expr(Role::App, 2, code).unwrap();
    prop.set_f32(Role::App, 3, EDITCHAT_HEIGHT).unwrap();

    chatview_rect_prop.add_depend(&prop, 3, "editz_h");
    editbox_bg_rect_prop.add_depend(&prop, 3, "editz_h");

    node.set_property_f32(Role::App, "baseline", TEXTBAR_BASELINE).unwrap();
    node.set_property_f32(Role::App, "linespacing", EDITCHAT_LINESPACING).unwrap();
    node.set_property_f32(Role::App, "descent", TEXT_DESCENT).unwrap();
    node.set_property_f32(Role::App, "font_size", FONTSIZE).unwrap();
    //node.set_property_str(Role::App, "text", "hello king!😁🍆jelly 🍆1234").unwrap();
    let prop = node.get_property("text_color").unwrap();
    if LIGHTMODE {
        prop.set_f32(Role::App, 0, 0.).unwrap();
        prop.set_f32(Role::App, 1, 0.).unwrap();
        prop.set_f32(Role::App, 2, 0.).unwrap();
        prop.set_f32(Role::App, 3, 1.).unwrap();
    } else {
        prop.set_f32(Role::App, 0, 1.).unwrap();
        prop.set_f32(Role::App, 1, 1.).unwrap();
        prop.set_f32(Role::App, 2, 1.).unwrap();
        prop.set_f32(Role::App, 3, 1.).unwrap();
    }
    let prop = node.get_property("text_hi_color").unwrap();
    prop.set_f32(Role::App, 0, 0.44).unwrap();
    prop.set_f32(Role::App, 1, 0.96).unwrap();
    prop.set_f32(Role::App, 2, 1.).unwrap();
    prop.set_f32(Role::App, 3, 1.).unwrap();
    let prop = node.get_property("cursor_color").unwrap();
    prop.set_f32(Role::App, 0, 0.816).unwrap();
    prop.set_f32(Role::App, 1, 0.627).unwrap();
    prop.set_f32(Role::App, 2, 1.).unwrap();
    prop.set_f32(Role::App, 3, 1.).unwrap();
    node.set_property_f32(Role::App, "cursor_ascent", EDITCHAT_CURSOR_ASCENT).unwrap();
    node.set_property_f32(Role::App, "cursor_descent", EDITCHAT_CURSOR_DESCENT).unwrap();
    let prop = node.get_property("hi_bg_color").unwrap();
    node.set_property_f32(Role::App, "select_ascent", EDITCHAT_SELECT_ASCENT).unwrap();
    node.set_property_f32(Role::App, "select_descent", EDITCHAT_SELECT_DESCENT).unwrap();
    node.set_property_f32(Role::App, "handle_descent", EDITCHAT_HANDLE_DESCENT).unwrap();
    if LIGHTMODE {
        prop.set_f32(Role::App, 0, 0.5).unwrap();
        prop.set_f32(Role::App, 1, 0.5).unwrap();
        prop.set_f32(Role::App, 2, 0.5).unwrap();
        prop.set_f32(Role::App, 3, 1.).unwrap();
    } else {
        prop.set_f32(Role::App, 0, 0.).unwrap();
        prop.set_f32(Role::App, 1, 0.27).unwrap();
        prop.set_f32(Role::App, 2, 0.22).unwrap();
        prop.set_f32(Role::App, 3, 1.).unwrap();
    }
    let prop = node.get_property("selected").unwrap();
    prop.set_null(Role::App, 0).unwrap();
    prop.set_null(Role::App, 1).unwrap();
    node.set_property_u32(Role::App, "z_index", 3).unwrap();
    //node.set_property_bool(Role::App, "debug", true).unwrap();

    //let editbox_text = PropertyStr::wrap(node, Role::App, "text", 0).unwrap();
    //let editbox_focus = PropertyBool::wrap(node, Role::App, "is_focused", 0).unwrap();
    //let darkirc_backend = app.darkirc_backend.clone();
    //let task = app.ex.spawn(async move {
    //    while let Ok(_) = btn_click_recvr.recv().await {
    //        let text = editbox_text.get();
    //        editbox_text.prop().unset(Role::App, 0).unwrap();
    //        // Clicking outside the editbox makes it lose focus
    //        // So lets focus it again
    //        editbox_focus.set(true);

    //        debug!(target: "app", "sending text {text}");

    //        let privmsg =
    //            Privmsg { channel: "#random".to_string(), nick: "king".to_string(), msg: text };
    //        darkirc_backend.send(privmsg).await;
    //    }
    //});
    //tasks.push(task);

    let node = node
        .setup(|me| {
            ChatEdit::new(
                me,
                window_scale.clone(),
                app.render_api.clone(),
                app.text_shaper.clone(),
                app.ex.clone(),
            )
        })
        .await;
    layer_node.clone().link(node);

    // Create the send button
    let node = create_button("send_btn");
    node.set_property_bool(Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("w - SENDLABEL_WIDTH").unwrap();
    prop.set_expr(Role::App, 0, code).unwrap();
    let code = cc.compile("h - EDITCHAT_HEIGHT").unwrap();
    prop.set_expr(Role::App, 1, code).unwrap();
    prop.set_f32(Role::App, 2, SENDLABEL_WIDTH).unwrap();
    prop.set_f32(Role::App, 3, EDITCHAT_HEIGHT).unwrap();

    let node = node.setup(|me| Button::new(me, app.ex.clone())).await;
    layer_node.clone().link(node);
}
