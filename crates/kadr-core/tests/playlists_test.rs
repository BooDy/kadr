use kadr_core::ast::{CardViewModel, QueryMacro, WidgetQueryBinding};
use kadr_core::events::SystemEvent;
use kadr_core::playlists::{
    AddFolderItemsSpec, AddPlaylistItemsRequest, AddPlaylistItemsResponse, AddShowItemsSpec,
    CreatePlaylistRequest, Playlist, PlaylistDetail, PlaylistItemView, PlaylistSummary,
    PlaylistsListResponse, ReorderPlaylistItemsRequest, UpdatePlaylistRequest,
};

#[test]
fn playlist_summary_and_detail_serialize_exact_wire_fields() {
    let pl = Playlist {
        id: "6f9d8c12-3a4b-4e5f-8c9d-0e1f2a3b4c5d".to_string(),
        owner_user_id: "u-123".to_string(),
        name: "Sci-Fi Marathon".to_string(),
        description: None,
        is_public: true,
        created_at: 1762940000,
        updated_at: 1762941800,
    };
    let pl_json = serde_json::to_value(&pl).expect("serialize playlist");
    assert!(pl_json.get("description").is_none());
    assert_eq!(pl_json["is_public"], true);

    let summary = PlaylistSummary {
        id: "6f9d8c12-3a4b-4e5f-8c9d-0e1f2a3b4c5d".to_string(),
        name: "Sci-Fi Marathon".to_string(),
        description: None,
        owner_user_id: "u-123".to_string(),
        owner_username: "alex".to_string(),
        is_public: true,
        is_editable: true,
        visible_item_count: 1,
        locked_item_count: 2,
        total_duration_seconds: 8880,
        cover_poster_urls: vec!["/api/v1/artwork/10/poster".to_string()],
        created_at: 1762940000,
        updated_at: 1762941800,
        contains_queried_item: None,
    };

    let summary_json = serde_json::to_value(&summary).expect("serialize summary");
    assert!(summary_json.get("description").is_none());
    assert!(summary_json.get("contains_queried_item").is_none());
    assert_eq!(summary_json["locked_item_count"], 2);

    let detail = PlaylistDetail {
        summary: PlaylistSummary {
            contains_queried_item: Some(true),
            ..summary
        },
        items: vec![PlaylistItemView {
            position: 0,
            added_at: 1762940010,
            duration_seconds: 8880,
            resume_position_seconds: Some(1240),
            card: CardViewModel {
                id: 10,
                title: "Arrival".to_string(),
                subtitle: Some("2016 • 4K".to_string()),
                poster_url: Some("/api/v1/artwork/10/poster".to_string()),
                backdrop_url: Some("/api/v1/artwork/10/backdrop".to_string()),
                media_type: "movie".to_string(),
                playback_progress: Some(0.14),
                rating: Some(7.9),
                release_year: Some(2016),
                badge: Some("RESUME".to_string()),
                season: None,
                episode: None,
            },
        }],
    };

    let encoded = serde_json::to_string(&detail).expect("serialize detail");
    let decoded: PlaylistDetail = serde_json::from_str(&encoded).expect("deserialize detail");
    assert_eq!(decoded, detail);
    assert_eq!(decoded.summary.contains_queried_item, Some(true));
    assert_eq!(decoded.items[0].card.id, 10);

    let list = PlaylistsListResponse {
        playlists: vec![decoded.summary.clone()],
    };
    let list_val = serde_json::to_value(&list).expect("serialize list");
    assert_eq!(
        list_val["playlists"][0]["id"],
        "6f9d8c12-3a4b-4e5f-8c9d-0e1f2a3b4c5d"
    );
}

#[test]
fn playlist_request_and_response_payloads_round_trip() {
    let create_req: CreatePlaylistRequest = serde_json::from_str(
        r#"{
            "name": "Weekend Queue",
            "description": "4K cuts",
            "is_public": true,
            "items": {
                "item_ids": [10, 14],
                "show": { "show_id": 50, "season_number": 1 },
                "folder": { "library_id": "lib-1", "path": "Season 01", "recursive": true }
            }
        }"#,
    )
    .expect("deserialize CreatePlaylistRequest");

    assert_eq!(create_req.name, "Weekend Queue");
    assert!(create_req.is_public);
    let items = create_req.items.expect("items present");
    assert_eq!(items.item_ids, Some(vec![10, 14]));
    assert_eq!(
        items.show,
        Some(AddShowItemsSpec {
            show_id: 50,
            season_number: Some(1),
        })
    );
    assert_eq!(
        items.folder,
        Some(AddFolderItemsSpec {
            library_id: "lib-1".to_string(),
            path: "Season 01".to_string(),
            recursive: true,
        })
    );

    // Defaults when is_public / recursive are omitted or null
    let minimal_create: CreatePlaylistRequest = serde_json::from_str(
        r#"{
            "name": "Minimal",
            "is_public": null,
            "items": {
                "folder": { "library_id": "lib-1", "path": "Movies" }
            }
        }"#,
    )
    .expect("deserialize minimal CreatePlaylistRequest");
    assert!(!minimal_create.is_public);
    assert!(!minimal_create.items.unwrap().folder.unwrap().recursive);

    let patch_req: UpdatePlaylistRequest =
        serde_json::from_str(r#"{"description": ""}"#).expect("deserialize UpdatePlaylistRequest");
    assert_eq!(patch_req.name, None);
    assert_eq!(patch_req.description.as_deref(), Some(""));
    assert_eq!(patch_req.is_public, None);

    let reorder_req: ReorderPlaylistItemsRequest =
        serde_json::from_str(r#"{"item_ids": [14, 10]}"#).expect("deserialize reorder");
    assert_eq!(reorder_req.item_ids, vec![14, 10]);

    let add_req = AddPlaylistItemsRequest {
        item_ids: Some(vec![1]),
        show: None,
        folder: None,
    };
    assert!(!add_req.is_empty());

    let empty_ids_req = AddPlaylistItemsRequest {
        item_ids: Some(vec![]),
        show: None,
        folder: None,
    };
    assert!(empty_ids_req.is_empty());

    let empty_req = AddPlaylistItemsRequest::default();
    assert!(empty_req.is_empty());

    let add_res = AddPlaylistItemsResponse {
        added_count: 2,
        skipped_duplicate_count: 1,
        summary: PlaylistSummary {
            id: "pl-1".to_string(),
            name: "Queue".to_string(),
            description: None,
            owner_user_id: "u-1".to_string(),
            owner_username: "admin".to_string(),
            is_public: false,
            is_editable: true,
            visible_item_count: 3,
            locked_item_count: 0,
            total_duration_seconds: 3600,
            cover_poster_urls: vec![],
            created_at: 100,
            updated_at: 200,
            contains_queried_item: None,
        },
    };
    let val = serde_json::to_value(&add_res).unwrap();
    assert_eq!(val["added_count"], 2);
    assert_eq!(val["skipped_duplicate_count"], 1);
}

#[test]
fn query_macro_playlist_items_and_playlist_updated_event_serialize() {
    let binding = WidgetQueryBinding::new(QueryMacro::PlaylistItems {
        playlist_id: "pl-uuid-1".to_string(),
    });
    let json = serde_json::to_value(&binding).expect("serialize binding");
    assert_eq!(
        json["macro_type"],
        serde_json::json!({ "playlist_items": { "playlist_id": "pl-uuid-1" } })
    );
    let parsed: WidgetQueryBinding = serde_json::from_value(json).expect("deserialize binding");
    assert_eq!(parsed, binding);

    let event = SystemEvent::PlaylistUpdated {
        playlist_id: "pl-uuid-1".to_string(),
        user_id: "u-1".to_string(),
        action: "items_changed".to_string(),
        timestamp: 1762942100,
    };
    assert_eq!(event.event_type(), "playlist:updated");
    let event_json = serde_json::to_value(&event).expect("serialize event");
    assert_eq!(event_json["type"], "playlist:updated");
    assert_eq!(event_json["payload"]["playlist_id"], "pl-uuid-1");
    assert_eq!(event_json["payload"]["user_id"], "u-1");
    assert_eq!(event_json["payload"]["action"], "items_changed");
    assert_eq!(event_json["payload"]["timestamp"], 1762942100);
}
