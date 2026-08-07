use serde::Serialize;
use sukaku_forge_app::port::{
    ApplicationPort, CommandDto, EngineDto, NextHintOutcomeDto, PROTOCOL_VERSION, RequestDto,
    ResponseDto, ResponseKindDto, VariantDto,
};

const PUZZLE: &str =
    "12345678.........................................................................";

#[derive(Serialize)]
struct GoldenFixture {
    protocol_version: u16,
    scenario: &'static str,
    steps: Vec<GoldenStep>,
}

#[derive(Serialize)]
struct GoldenStep {
    request: RequestDto,
    response: ResponseDto,
}

#[must_use]
pub fn render() -> String {
    let mut port = ApplicationPort::new();
    let mut steps = Vec::with_capacity(3);

    let create_request = RequestDto {
        protocol_version: PROTOCOL_VERSION,
        request_id: 1,
        command: CommandDto::CreateSession {
            puzzle: PUZZLE.to_owned(),
            variant: VariantDto::default(),
            engine: EngineDto::default(),
        },
    };
    let create_response = port.dispatch(create_request.clone());
    let revision = match &create_response.response {
        ResponseKindDto::SessionCreated { snapshot, .. } => snapshot.revision.clone(),
        response => panic!("create_session returned {response:?}"),
    };
    steps.push(GoldenStep {
        request: create_request,
        response: create_response,
    });

    let next_request = RequestDto {
        protocol_version: PROTOCOL_VERSION,
        request_id: 2,
        command: CommandDto::NextHint {
            expected_revision: revision,
        },
    };
    let next_response = port.dispatch(next_request.clone());
    let (revision, hint_id) = match &next_response.response {
        ResponseKindDto::NextHint {
            revision,
            outcome: NextHintOutcomeDto::Presented { hint_id, .. },
        } => (revision.clone(), hint_id.clone()),
        response => panic!("next_hint returned {response:?}"),
    };
    steps.push(GoldenStep {
        request: next_request,
        response: next_response,
    });

    let apply_request = RequestDto {
        protocol_version: PROTOCOL_VERSION,
        request_id: 3,
        command: CommandDto::ApplyHint {
            expected_revision: revision,
            hint_id,
        },
    };
    let apply_response = port.dispatch(apply_request.clone());
    assert!(
        matches!(&apply_response.response, ResponseKindDto::Snapshot { .. }),
        "apply_hint must finish the golden sequence"
    );
    steps.push(GoldenStep {
        request: apply_request,
        response: apply_response,
    });

    let fixture = GoldenFixture {
        protocol_version: PROTOCOL_VERSION,
        scenario: "hidden_single_round_trip",
        steps,
    };
    let mut rendered = serde_json::to_string_pretty(&fixture).expect("golden fixture serializes");
    rendered.push('\n');
    rendered
}
