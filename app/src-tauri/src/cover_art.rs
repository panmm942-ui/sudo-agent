//! Album images returned by Spotify itself. Never synthesize CDN image URLs.
use serde::{Deserialize,Serialize};
use serde_json::Value;
use std::{collections::VecDeque,io::Read,sync::{Mutex,OnceLock},time::{Duration,Instant}};

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase")]
pub struct CoverArt {pub url:String,pub width:Option<u32>,pub height:Option<u32>,pub spotify_url:String,pub track_id:String,pub source_url:String}
#[derive(Clone)]
struct Candidate {item:Value,cover:Option<CoverArt>,until:Instant}
fn candidates()->&'static Mutex<VecDeque<Candidate>>{static CACHE:OnceLock<Mutex<VecDeque<Candidate>>>=OnceLock::new();CACHE.get_or_init(||Mutex::new(VecDeque::new()))}
fn valid_id(value:&str)->bool{value.len()==22&&value.bytes().all(|byte|byte.is_ascii_alphanumeric())}
fn valid_url(value:&str)->bool{value.len()<=2048&&reqwest::Url::parse(value).is_ok_and(|url|url.scheme()=="https"&&url.host_str()==Some("i.scdn.co")&&url.port().is_none()&&url.username().is_empty()&&url.password().is_none()&&url.fragment().is_none()&&url.query().is_none()&&url.path().starts_with("/image/")&&url.path().len()>7)}
fn dimension(value:&Value)->Option<u32>{value.as_u64().filter(|value|*value>0&&*value<=16384).map(|value|value as u32)}
fn largest_image(item:&Value)->Option<CoverArt>{
    let id=item["id"].as_str().filter(|id|valid_id(id))?;
    let images=item["album"]["images"].as_array()?;
    let image=images.iter().filter(|image|image["url"].as_str().is_some_and(valid_url)).max_by_key(|image|(dimension(&image["width"]).unwrap_or(0),dimension(&image["height"]).unwrap_or(0)))?;
    Some(CoverArt{url:image["url"].as_str()?.into(),width:dimension(&image["width"]),height:dimension(&image["height"]),spotify_url:format!("https://open.spotify.com/track/{id}"),track_id:id.into(),source_url:image["url"].as_str()?.into()})
}
fn matches_metadata(item:&Value,title:&str,artist:&str,album:Option<&str>,duration:Option<f64>)->bool{
    let Some(album)=album.filter(|value|!value.trim().is_empty())else{return false;};
    let Ok(track)=serde_json::from_value::<crate::spotify::Track>(serde_json::json!({"title":title,"artist":artist,"album":album,"duration":duration}))else{return false;};
    crate::spotify::actions::matching_uri(item,&track).is_some()
}
/// Reuse catalogue metadata already returned for another feature, without
/// making an extra request. The cache is bounded and contains no credentials.
pub(crate) fn remember_item(item:&Value){
    let Some(title)=item["name"].as_str()else{return;};
    let Some(artists)=item["artists"].as_array().and_then(|values|values.iter().map(|value|value["name"].as_str()).collect::<Option<Vec<_>>>())else{return;};
    if !matches_metadata(item,title,&artists.join(", "),item["album"]["name"].as_str(),item["duration_ms"].as_f64().map(|value|value/1000.0))||item.to_string().len()>64_000{return;}
    if let Ok(mut cache)=candidates().lock(){
        let now=Instant::now();cache.retain(|value|value.until>now&&value.item["id"]!=item["id"]);
        if cache.len()>=32{cache.pop_front();}
        cache.push_back(Candidate{item:item.clone(),cover:largest_image(item),until:now+Duration::from_secs(86_400)});
    }
}
fn cached_item(title:&str,artist:&str,album:Option<&str>,duration:Option<f64>)->Option<Candidate>{
    candidates().lock().ok()?.iter().rev().find(|value|value.until>Instant::now()&&matches_metadata(&value.item,title,artist,album,duration)).cloned()
}
pub(crate) fn matched_track_id(title:&str,artist:&str,album:Option<&str>,duration:Option<f64>)->Option<String>{cached_item(title,artist,album,duration)?.item["id"].as_str().map(str::to_owned)}
fn guarded_lookup(guard:impl Fn()->Result<(),String>,lookup:impl FnOnce()->Result<Option<CoverArt>,String>)->Result<Option<CoverArt>,String>{guard()?;let value=lookup()?;guard()?;Ok(value)}
fn after_ready(ready:impl FnOnce()->Result<(),String>,request:impl FnOnce()->Result<Option<CoverArt>,String>)->Result<Option<CoverArt>,String>{ready()?;request()}
fn larger_variant(url:&str)->Option<String>{
    if !valid_url(url){return None;}
    let image=url.strip_prefix("https://i.scdn.co/image/ab67616d0000")?;
    let hash=image.strip_prefix("b273").or_else(||image.strip_prefix("1e02"))?;
    if hash.len()!=24||!hash.bytes().all(|b|b.is_ascii_hexdigit()){return None;}
    Some(format!("https://i.scdn.co/image/ab67616d000082c1{hash}"))
}
fn image_dimensions(bytes:&[u8])->Option<(u32,u32)>{
    if bytes.starts_with(&[137,80,78,71,13,10,26,10])&&bytes.len()>=24{return Some((u32::from_be_bytes(bytes[16..20].try_into().ok()?),u32::from_be_bytes(bytes[20..24].try_into().ok()?)));}
    if !bytes.starts_with(&[255,216]){return None;}
    let mut at=2;
    while at+4<=bytes.len(){
        if bytes[at]!=255{return None;}while at<bytes.len()&&bytes[at]==255{at+=1;}
        let marker=*bytes.get(at)?;at+=1;
        if marker==0xda||marker==0xd9{return None;}
        if marker==0x01||(0xd0..=0xd8).contains(&marker){continue;}
        let len=u16::from_be_bytes(bytes.get(at..at+2)?.try_into().ok()?) as usize;
        if len<2||at+len>bytes.len(){return None;}
        if [0xc0,0xc1,0xc2,0xc3,0xc5,0xc6,0xc7,0xc9,0xca,0xcb,0xcd,0xce,0xcf].contains(&marker)&&len>=7{
            return Some((u16::from_be_bytes(bytes[at+5..at+7].try_into().ok()?)as u32,u16::from_be_bytes(bytes[at+3..at+5].try_into().ok()?)as u32));
        }at+=len;
    }None
}
#[cfg(windows)]
fn image_bytes(client:&reqwest::blocking::Client,url:&str)->Result<Vec<u8>,String>{
    let response=client.get(url).send().map_err(|_|"Spotify cover image could not load.")?;
    if !response.status().is_success()||response.content_length().is_some_and(|size|size>4_194_304){return Err("Spotify cover image was unavailable or too large.".into());}
    let mut bytes=Vec::new();response.take(4_194_305).read_to_end(&mut bytes).map_err(|_|"Could not read Spotify cover image.")?;
    if bytes.len()>4_194_304{return Err("Spotify cover image was too large.".into());}Ok(bytes)
}
#[cfg(windows)]
fn materialize(mut cover:CoverArt)->Result<CoverArt,String>{
    use base64::Engine;
    static IMAGES:OnceLock<Mutex<VecDeque<(String,CoverArt)>>>=OnceLock::new();let images=IMAGES.get_or_init(||Mutex::new(VecDeque::new()));
    let original_url=cover.source_url.clone();
    if let Some((_,cached))=images.lock().map_err(|_|"Spotify image cache is unavailable.")?.iter().find(|(source,_)|source==&original_url).cloned(){return Ok(CoverArt{url:cached.url,source_url:cached.source_url,width:cached.width,height:cached.height,..cover});}
    if !valid_url(&cover.source_url){return Err("Spotify returned an unsupported cover URL.".into());}
    // Public image CDN only. This client never receives account credentials.
    let client=reqwest::blocking::Client::builder().timeout(Duration::from_secs(5)).connect_timeout(Duration::from_secs(3)).redirect(reqwest::redirect::Policy::none()).no_proxy().build().map_err(|_|"Could not initialize cover image download.")?;
    let mut bytes=image_bytes(&client,&cover.source_url)?;
    let mut dimensions=image_dimensions(&bytes);
    // This public CDN variant is unofficial: accept it only when its original
    // bytes actually decode to a larger image, retaining the exact artwork ID.
    if let Some(candidate)=larger_variant(&cover.source_url){
        if let Ok(original)=image_bytes(&client,&candidate){
            if let (Some((w,h)),Some((base_w,base_h)))=(image_dimensions(&original),dimensions){
                if w>base_w&&h>base_h&&w<=16384&&h<=16384{bytes=original;dimensions=Some((w,h));cover.source_url=candidate;}
            }
        }
    }
    if let Some((w,h))=dimensions{cover.width=Some(w);cover.height=Some(h);}
    let mime=match bytes.as_slice(){[255,216,255,..]=>"image/jpeg",[137,80,78,71,13,10,26,10,..]=>"image/png",[b'R',b'I',b'F',b'F',_,_,_,_,b'W',b'E',b'B',b'P',..]=>"image/webp",_=>return Err("Spotify returned an unsupported cover image.".into())};
    cover.url=format!("data:{mime};base64,{}",base64::engine::general_purpose::STANDARD.encode(bytes));
    let mut cache=images.lock().map_err(|_|"Spotify image cache is unavailable.")?;if cache.len()>=8{cache.pop_front();}cache.push_back((original_url,cover.clone()));Ok(cover)
}
#[cfg(not(windows))]
fn materialize(_:CoverArt)->Result<CoverArt,String>{Err("Spotify covers require the Windows app.".into())}
fn response(response:reqwest::blocking::Response)->Result<Value,String>{
    if response.status().as_u16()==429{crate::canvas::record_spotify_web_api_rate_limit(response.headers().get(reqwest::header::RETRY_AFTER).and_then(|value|value.to_str().ok()))?;}
    if response.status().as_u16()==204{return Ok(Value::Null);}
    if !response.status().is_success(){return Err(match response.status().as_u16(){401=>"Spotify cover connection expired.",403=>"Spotify did not permit the current cover lookup.",_=>"Spotify cover art is unavailable."}.into());}
    if response.content_length().is_some_and(|size|size>256_000){return Err("Spotify cover metadata was too large.".into());}
    let mut bytes=Vec::new();response.take(256_001).read_to_end(&mut bytes).map_err(|_|"Could not read Spotify cover metadata.")?;
    if bytes.len()>256_000{return Err("Spotify cover metadata was too large.".into());}
    serde_json::from_slice(&bytes).map_err(|_|"Spotify returned unreadable cover metadata.".into())
}

#[tauri::command]
pub async fn spotify_cover_art(track_key:String,title:String,artist:String,album:Option<String>,duration:Option<f64>)->Result<Option<CoverArt>,String>{
    if track_key.is_empty()||track_key.len()>2000||title.trim().is_empty()||title.len()>2000||artist.trim().is_empty()||artist.len()>2000||album.as_ref().is_none_or(|value|value.trim().is_empty()||value.len()>2000)||duration.is_none_or(|value|!value.is_finite()||value<=0.0||value>86_400.0){return Ok(None);}
    tauri::async_runtime::spawn_blocking(move||{
        let guard=||{
            let native=crate::spotify::current_native_track(&track_key)?;
            let value=serde_json::to_value(native).map_err(|_|"Could not verify Spotify cover identity.")?;
            if value["title"].as_str()!=Some(title.as_str())||value["artist"].as_str()!=Some(artist.as_str())||value["album"].as_str()!=album.as_deref()||value["duration"].as_f64()!=duration{return Err("Spotify changed songs before loading its cover.".into());}
            Ok(())
        };
        guarded_lookup(&guard,||{
            if let Some(packet)=crate::spotify_bridge::matching_snapshot(&track_key,&title,&artist,album.as_deref(),duration){
                if let Some(image)=packet.images.into_iter().next(){
                    return materialize(CoverArt{url:image.url.clone(),source_url:image.url,width:image.width,height:image.height,spotify_url:format!("https://open.spotify.com/track/{}",packet.track_id),track_id:packet.track_id}).map(Some);
                }
            }
            if let Some(cached)=cached_item(&title,&artist,album.as_deref(),duration){return cached.cover.map(materialize).transpose();}
            // Share the player lane and global cooldown. Check before token
            // retrieval, so a saved cookie cannot trigger work during backoff.
            let _request=crate::spotify::actions::PLAYER_REQUEST.lock().map_err(|_|"Spotify cover lookup is unavailable.")?;
            guard()?;
            if let Some(cached)=cached_item(&title,&artist,album.as_deref(),duration){return cached.cover.map(materialize).transpose();}
            after_ready(crate::canvas::spotify_web_api_ready,||{
                let revision=crate::canvas::connection_revision();
                let client=reqwest::blocking::Client::builder().timeout(Duration::from_secs(5)).connect_timeout(Duration::from_secs(3)).redirect(reqwest::redirect::Policy::none()).no_proxy().build().map_err(|_|"Could not initialize Spotify cover lookup.")?;
                let token=crate::canvas::library_access_token(&client)?;
                crate::spotify::actions::connection_current(revision)?;
                crate::canvas::spotify_web_api_ready()?;
                let data=response(client.get("https://api.spotify.com/v1/me/player/currently-playing").bearer_auth(token).send().map_err(|_|"Spotify cover lookup timed out.")?)?;
                crate::spotify::actions::connection_current(revision)?;
                let item=&data["item"];
                if !matches_metadata(item,&title,&artist,album.as_deref(),duration){return Ok(None);}
                remember_item(item);largest_image(item).map(materialize).transpose()
            })
        })
    }).await.map_err(|_|"Spotify cover worker failed.".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]fn larger_variant_preserves_the_artwork_identity_and_requires_actual_pixels(){
        let hash="bc18bdade69ec5ef0bb25b17";
        assert_eq!(larger_variant(&format!("https://i.scdn.co/image/ab67616d0000b273{hash}")),Some(format!("https://i.scdn.co/image/ab67616d000082c1{hash}")));
        assert!(larger_variant("https://evil.test/image/ab67616d0000b273bc18bdade69ec5ef0bb25b17").is_none());
        assert!(image_dimensions(&[255,216,255]).is_none());
        assert_eq!(image_dimensions(&[255,216,255,192,0,8,8,5,220,5,220,0]),Some((1500,1500)));
    }
    const ID:&str="3OHfY25tqY28d16oZczHc8";
    fn item()->Value{serde_json::json!({"id":ID,"uri":format!("spotify:track:{ID}"),"type":"track","name":"Song","artists":[{"name":"Artist"}],"album":{"name":"Album","images":[{"url":"https://i.scdn.co/image/small","width":64,"height":64},{"url":"https://i.scdn.co/image/large","width":640,"height":640},{"url":"https://i.scdn.co/image/medium","width":300,"height":300}]},"duration_ms":180000})}
    #[test]fn selects_the_largest_original_url_instead_of_assuming_array_order_or_rewriting_hashes(){
        let result=largest_image(&item()).unwrap();assert_eq!(result.url,"https://i.scdn.co/image/large");assert_eq!((result.width,result.height),(Some(640),Some(640)));assert_eq!(result.spotify_url,format!("https://open.spotify.com/track/{ID}"));
    }
    #[test]fn rejects_unsafe_images_and_does_not_make_up_a_larger_size(){
        let mut data=item();data["album"]["images"]=serde_json::json!([{"url":"https://evil.test/large","width":4000,"height":4000},{"url":"https://i.scdn.co/image/original","width":300,"height":300}]);assert_eq!(largest_image(&data).unwrap().url,"https://i.scdn.co/image/original");
        for value in ["http://i.scdn.co/image/x","https://i.scdn.co.evil.test/image/x","https://i.scdn.co/image/x?size=4000","https://user@i.scdn.co/image/x"]{assert!(!valid_url(value));}
    }
    #[test]fn cached_catalogue_items_require_the_full_native_song_identity(){
        let data=item();assert!(matches_metadata(&data,"Song","Artist",Some("Album"),Some(180.0)));
        for (title,artist,album,duration) in [("Song Live","Artist",Some("Album"),Some(180.0)),("Song","Other",Some("Album"),Some(180.0)),("Song","Artist",Some("Compilation"),Some(180.0)),("Song","Artist",Some("Album"),Some(185.0)),("Song","Artist",None,Some(180.0))]{assert!(!matches_metadata(&data,title,artist,album,duration));}
    }
    #[test]fn a_current_native_song_change_discards_the_lookup_before_publication(){
        let checks=std::cell::Cell::new(0);let calls=std::cell::Cell::new(0);
        let result=guarded_lookup(||{checks.set(checks.get()+1);if checks.get()>1{Err("song changed".into())}else{Ok(())}},||{calls.set(calls.get()+1);Ok(Some(largest_image(&item()).unwrap()))});
        assert_eq!(result,Err("song changed".into()));assert_eq!(calls.get(),1);
    }
    #[test]fn cooldown_blocks_before_any_token_or_api_work(){
        let requests=std::cell::Cell::new(0);
        let result=after_ready(||Err("cooldown".into()),||{requests.set(requests.get()+1);Ok(None)});
        assert_eq!(result,Err("cooldown".into()));assert_eq!(requests.get(),0);
    }
}
