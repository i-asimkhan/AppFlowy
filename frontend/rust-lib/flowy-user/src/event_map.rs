use std::sync::{Arc, Weak};

use collab_folder::core::FolderData;
use strum_macros::Display;

use flowy_derive::{Flowy_Event, ProtoBuf_Enum};
use flowy_error::FlowyResult;
use flowy_server_config::supabase_config::SupabaseConfiguration;
use flowy_user_deps::cloud::UserService;
use flowy_user_deps::entities::*;
use lib_dispatch::prelude::*;
use lib_infra::future::{to_fut, Fut};

use crate::event_handler::*;
use crate::{errors::FlowyError, services::UserSession};

pub fn init(user_session: Weak<UserSession>) -> AFPlugin {
  let store_preferences = user_session
    .upgrade()
    .map(|session| session.get_store_preferences())
    .unwrap();
  AFPlugin::new()
    .name("Flowy-User")
    .state(user_session)
    .state(store_preferences)
    .event(UserEvent::SignIn, sign_in)
    .event(UserEvent::SignUp, sign_up)
    .event(UserEvent::InitUser, init_user_handler)
    .event(UserEvent::GetUserProfile, get_user_profile_handler)
    .event(UserEvent::SignOut, sign_out)
    .event(UserEvent::UpdateUserProfile, update_user_profile_handler)
    .event(UserEvent::CheckUser, check_user_handler)
    .event(UserEvent::SetAppearanceSetting, set_appearance_setting)
    .event(UserEvent::GetAppearanceSetting, get_appearance_setting)
    .event(UserEvent::GetUserSetting, get_user_setting)
    .event(UserEvent::SetSupabaseConfig, set_supabase_config_handler)
    .event(UserEvent::GetSupabaseConfig, get_supabase_config_handler)
    .event(UserEvent::ThirdPartyAuth, third_party_auth_handler)
    .event(
      UserEvent::GetAllUserWorkspaces,
      get_all_user_workspace_handler,
    )
    .event(UserEvent::OpenWorkspace, open_workspace_handler)
    .event(UserEvent::AddUserToWorkspace, add_user_to_workspace_handler)
    .event(
      UserEvent::RemoveUserToWorkspace,
      remove_user_from_workspace_handler,
    )
    .event(UserEvent::UpdateNetworkState, update_network_state_handler)
    .event(UserEvent::GetHistoricalUsers, get_historical_users_handler)
    .event(UserEvent::OpenHistoricalUser, open_historical_users_handler)
}

pub struct SignUpContext {
  /// Indicate whether the user is new or not.
  pub is_new: bool,
  /// If the user is sign in as guest, and the is_new is true, then the folder data will be not
  /// None.
  pub local_folder: Option<FolderData>,
}

pub trait UserStatusCallback: Send + Sync + 'static {
  /// When the [AuthType] changed, this method will be called. Currently, the auth type
  /// will be changed when the user sign in or sign up.
  fn auth_type_did_changed(&self, auth_type: AuthType);
  /// This will be called after the application launches if the user is already signed in.
  /// If the user is not signed in, this method will not be called
  fn did_init(&self, user_id: i64, user_workspace: &UserWorkspace) -> Fut<FlowyResult<()>>;
  /// Will be called after the user signed in.
  fn did_sign_in(&self, user_id: i64, user_workspace: &UserWorkspace) -> Fut<FlowyResult<()>>;
  /// Will be called after the user signed up.
  fn did_sign_up(
    &self,
    context: SignUpContext,
    user_profile: &UserProfile,
    user_workspace: &UserWorkspace,
  ) -> Fut<FlowyResult<()>>;

  fn did_expired(&self, token: &str, user_id: i64) -> Fut<FlowyResult<()>>;
  fn open_workspace(&self, user_id: i64, user_workspace: &UserWorkspace) -> Fut<FlowyResult<()>>;
  fn did_update_network(&self, reachable: bool);
}

/// The user cloud service provider.
/// The provider can be supabase, firebase, aws, or any other cloud service.
pub trait UserCloudServiceProvider: Send + Sync + 'static {
  fn update_supabase_config(&self, supabase_config: &SupabaseConfiguration);
  fn set_auth_type(&self, auth_type: AuthType);
  fn get_user_service(&self) -> Result<Arc<dyn UserService>, FlowyError>;
  fn service_name(&self) -> String;
}

impl<T> UserCloudServiceProvider for Arc<T>
where
  T: UserCloudServiceProvider,
{
  fn update_supabase_config(&self, supabase_config: &SupabaseConfiguration) {
    (**self).update_supabase_config(supabase_config)
  }

  fn set_auth_type(&self, auth_type: AuthType) {
    (**self).set_auth_type(auth_type)
  }

  fn get_user_service(&self) -> Result<Arc<dyn UserService>, FlowyError> {
    (**self).get_user_service()
  }

  fn service_name(&self) -> String {
    (**self).service_name()
  }
}

/// Acts as a placeholder [UserStatusCallback] for the user session, but does not perform any function
pub(crate) struct DefaultUserStatusCallback;
impl UserStatusCallback for DefaultUserStatusCallback {
  fn auth_type_did_changed(&self, _auth_type: AuthType) {}

  fn did_init(&self, _user_id: i64, _user_workspace: &UserWorkspace) -> Fut<FlowyResult<()>> {
    to_fut(async { Ok(()) })
  }

  fn did_sign_in(&self, _user_id: i64, _user_workspace: &UserWorkspace) -> Fut<FlowyResult<()>> {
    to_fut(async { Ok(()) })
  }

  fn did_sign_up(
    &self,
    _context: SignUpContext,
    _user_profile: &UserProfile,
    _user_workspace: &UserWorkspace,
  ) -> Fut<FlowyResult<()>> {
    to_fut(async { Ok(()) })
  }

  fn did_expired(&self, _token: &str, _user_id: i64) -> Fut<FlowyResult<()>> {
    to_fut(async { Ok(()) })
  }

  fn open_workspace(&self, _user_id: i64, _user_workspace: &UserWorkspace) -> Fut<FlowyResult<()>> {
    to_fut(async { Ok(()) })
  }

  fn did_update_network(&self, _reachable: bool) {}
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Display, Hash, ProtoBuf_Enum, Flowy_Event)]
#[event_err = "FlowyError"]
pub enum UserEvent {
  /// Only use when the [AuthType] is Local or SelfHosted
  /// Logging into an account using a register email and password
  #[event(input = "SignInPayloadPB", output = "UserProfilePB")]
  SignIn = 0,

  /// Only use when the [AuthType] is Local or SelfHosted
  /// Creating a new account
  #[event(input = "SignUpPayloadPB", output = "UserProfilePB")]
  SignUp = 1,

  /// Logging out fo an account
  #[event()]
  SignOut = 2,

  /// Update the user information
  #[event(input = "UpdateUserProfilePayloadPB")]
  UpdateUserProfile = 3,

  /// Get the user information
  #[event(output = "UserProfilePB")]
  GetUserProfile = 4,

  /// Check the user current session is valid or not
  #[event(output = "UserProfilePB")]
  CheckUser = 5,

  /// Initialize resources for the current user after launching the application
  #[event()]
  InitUser = 6,

  /// Change the visual elements of the interface, such as theme, font and more
  #[event(input = "AppearanceSettingsPB")]
  SetAppearanceSetting = 7,

  /// Get the appearance setting
  #[event(output = "AppearanceSettingsPB")]
  GetAppearanceSetting = 8,

  /// Get the settings of the user, such as the user storage folder
  #[event(output = "UserSettingPB")]
  GetUserSetting = 9,

  #[event(input = "ThirdPartyAuthPB", output = "UserProfilePB")]
  ThirdPartyAuth = 10,

  /// Set the supabase config. It will be written to the environment variables.
  /// Check out the `write_to_env` of [SupabaseConfigPB].
  #[event(input = "SupabaseConfigPB")]
  SetSupabaseConfig = 13,

  #[event(output = "SupabaseConfigPB")]
  GetSupabaseConfig = 14,

  /// Return the all the workspaces of the user
  #[event()]
  GetAllUserWorkspaces = 20,

  #[event(input = "UserWorkspacePB")]
  OpenWorkspace = 21,

  #[event(input = "AddWorkspaceUserPB")]
  AddUserToWorkspace = 22,

  #[event(input = "RemoveWorkspaceUserPB")]
  RemoveUserToWorkspace = 23,

  #[event(input = "NetworkStatePB")]
  UpdateNetworkState = 24,

  #[event(output = "RepeatedHistoricalUserPB")]
  GetHistoricalUsers = 25,

  #[event(input = "HistoricalUserPB")]
  OpenHistoricalUser = 26,
}
