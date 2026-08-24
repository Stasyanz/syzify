use tauri::State;
use uuid::Uuid;

use crate::db;
use crate::models::segment::{NewSegmentMeta, Segment, SegmentEffortRow, SimilarSegment};
use crate::models::trackpoint::TrackGeometry;
use crate::state::AppState;

/// Sport + geometry of the source activity (the only columns segment
/// building needs — not the full 20-column trackpoint read).
fn segment_source(
    conn: &rusqlite::Connection,
    activity_id: &str,
) -> Result<(String, TrackGeometry), String> {
    let sport = db::segments::activity_sport(conn, activity_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "activity not found".to_string())?;
    let geo = db::trackpoints::get_track_geometry(conn, activity_id).map_err(|e| e.to_string())?;
    Ok((sport, geo))
}

/// Existing segments that look like duplicates of the would-be segment
/// (same sport, both endpoints within ~50 m, length within ±10%). The UI
/// shows these as a warning before saving; saving stays allowed.
#[tauri::command]
pub fn check_similar_segments(
    activity_id: String,
    start_idx: usize,
    end_idx: usize,
    state: State<AppState>,
) -> Result<Vec<SimilarSegment>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let (sport, geo) = segment_source(&conn, &activity_id)?;
    // A throwaway build gives the candidate's endpoints and haversine length
    // with the same GPS filtering the save will apply.
    let (seg, _) = db::segments::build_segment(
        NewSegmentMeta {
            name: "",
            sport: &sport,
            activity_id: &activity_id,
            id: "",
            created_at: "",
        },
        start_idx,
        end_idx,
        &geo,
    )?;
    db::segments::find_similar(
        &conn,
        &sport,
        seg.start_lat,
        seg.start_lon,
        seg.end_lat,
        seg.end_lon,
        seg.distance_m,
    )
    .map_err(|e| e.to_string())
}

/// Save the selected trackpoint range as a named segment. Everything is
/// recomputed backend-side from the stored trackpoints — the frontend only
/// supplies the intent (activity + index range + name).
#[tauri::command]
pub fn save_segment(
    activity_id: String,
    start_idx: usize,
    end_idx: usize,
    name: String,
    state: State<AppState>,
) -> Result<Segment, String> {
    let name = db::segments::validated_name(&name)?;
    let mut conn = state.db.lock().map_err(|e| e.to_string())?;
    let (sport, geo) = segment_source(&conn, &activity_id)?;
    let id = Uuid::new_v4().to_string();
    let created_at = chrono::Utc::now().to_rfc3339();
    let (seg, points) = db::segments::build_segment(
        NewSegmentMeta {
            name,
            sport: &sport,
            activity_id: &activity_id,
            id: &id,
            created_at: &created_at,
        },
        start_idx,
        end_idx,
        &geo,
    )?;
    db::segments::insert_segment(&mut conn, &seg, &points).map_err(|e| e.to_string())?;
    drop(conn);
    // Backfill efforts over existing same-sport activities in a worker
    // thread — scanning a large vault takes seconds and must not pin the DB
    // mutex from a command. Per-activity locking keeps the UI responsive.
    backfill_segment_off_thread(state.db.clone(), seg.id.clone());
    Ok(seg)
}

/// Worker-thread backfill for a fresh segment: one short mutex hold per
/// activity, per-activity failures logged and skipped (matching is
/// idempotent, so any later rematch repairs a skipped pair).
fn backfill_segment_off_thread(db_handle: crate::state::Db, segment_id: String) {
    std::thread::spawn(move || {
        let act_ids = {
            let Ok(conn) = db_handle.lock() else { return };
            let sport = match db::segment_efforts::segment_sport(&conn, &segment_id) {
                Ok(Some(s)) => s,
                Ok(None) => return,
                Err(e) => {
                    eprintln!("Segment backfill: cannot read segment {segment_id}: {e}");
                    return;
                }
            };
            match db::segment_efforts::activity_ids_for_sport(&conn, &sport) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Segment backfill: cannot list activities: {e}");
                    return;
                }
            }
        };
        for aid in act_ids {
            let Ok(conn) = db_handle.lock() else { return };
            if let Err(e) = db::segment_efforts::match_pair(&conn, &segment_id, &aid) {
                eprintln!("Segment backfill: skipping activity {aid}: {e}");
            }
        }
    });
}

/// The activity's detected segment passes, with per-segment standings.
#[tauri::command]
pub fn get_activity_segment_efforts(
    activity_id: String,
    state: State<AppState>,
) -> Result<Vec<SegmentEffortRow>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::segment_efforts::efforts_for_activity(&conn, &activity_id).map_err(|e| e.to_string())
}
