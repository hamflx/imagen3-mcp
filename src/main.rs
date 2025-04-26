use base64::{self, Engine as _};
use chrono;
use directories::ProjectDirs;
use nanoid;
use reqwest;
use rmcp::{
    ServerHandler, ServiceExt,
    model::{Implementation, ServerCapabilities, ServerInfo},
    schemars, tool,
};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;
use warp::Filter;

#[derive(Debug, Clone)]
struct ImageGenerationServer {
    resources_path: PathBuf,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ImagePrompt {
    #[schemars(
        description = "The prompt text for image generation. The prompt MUST be in English."
    )]
    prompt: String,
}

// Request and response structures for the Gemini API
#[derive(Debug, Serialize)]
struct GeminiRequest {
    instances: Vec<GeminiInstance>,
    parameters: GeminiParameters,
}

#[derive(Debug, Serialize)]
struct GeminiInstance {
    prompt: String,
}

#[derive(Debug, Serialize)]
struct GeminiParameters {
    #[serde(rename = "sampleCount")]
    sample_count: i32,
}

#[derive(Debug, Deserialize)]
struct GeminiResponse {
    predictions: Vec<GeminiPrediction>,
}

#[derive(Debug, Deserialize)]
struct GeminiPrediction {
    #[serde(rename = "mimeType")]
    mime_type: String,
    #[serde(rename = "bytesBase64Encoded")]
    bytes_base64_encoded: String,
}

// Function to generate an image using the Gemini API
async fn generate_image_from_gemini(
    prompt: &str,
    resources_path: &PathBuf,
) -> Result<String, Box<dyn std::error::Error>> {
    // Generate a filename based on the prompt
    let timestamp = chrono::Local::now().format("%Y%m%d%H%M%S").to_string();
    let id = nanoid::nanoid!(10);
    let filename = format!("{}_{}.png", id, timestamp);
    let path = resources_path.join("images").join(&filename);

    // Get the API key from environment variables
    let api_key =
        env::var("GEMINI_API_KEY").map_err(|_| "GEMINI_API_KEY environment variable not set")?;

    // Create the request
    let request = GeminiRequest {
        instances: vec![GeminiInstance {
            prompt: prompt.to_string(),
        }],
        parameters: GeminiParameters {
            sample_count: 1, // Just generate one image
        },
    };

    // Create URL with API key
    let base_url = env::var("BASE_URL")
        .unwrap_or_else(|_| "https://generativelanguage.googleapis.com".to_string());
    let url = format!(
        "{}/v1beta/models/imagen-3.0-generate-002:predict?key={}",
        base_url, api_key
    );

    // Make the request
    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .json(&request)
        .send()
        .await?
        .text()
        .await?;
    let response: GeminiResponse = match serde_json::from_str(&response) {
        Ok(response) => response,
        Err(e) => {
            return Err(format!(
                "Failed to parse Gemini response: {}\nThe response was: {}",
                e, response
            )
            .into());
        }
    };

    // Make sure we got at least one prediction
    if response.predictions.is_empty() {
        return Err("No images were generated".into());
    }

    // Get the first prediction
    let prediction = &response.predictions[0];

    // Decode the base64 image using updated API
    let image_data =
        base64::engine::general_purpose::STANDARD.decode(&prediction.bytes_base64_encoded)?;

    // Write the image to disk
    fs::write(&path, &image_data)?;

    Ok(filename)
}

// Define the tool and its implementation
#[tool(tool_box)]
impl ImageGenerationServer {
    #[tool(
        description = "Generate an image based on a prompt. Returns an image URL that can be used in markdown format like ![description](URL) to display the image"
    )]
    async fn generate_image(&self, #[tool(aggr)] prompt: ImagePrompt) -> String {
        // Generate the image using the Gemini API

        match generate_image_from_gemini(&prompt.prompt, &self.resources_path).await {
            Ok(filename) => {
                // Return the URL to the generated image
                format!("http://127.0.0.1:9981/images/{}", filename)
            }
            Err(e) => {
                eprintln!("Error generating image: {}", e);
                format!("Error generating image: {}", e)
            }
        }
    }
}

// Implement ServerHandler trait for our image generation server
#[tool(tool_box)]
impl ServerHandler for ImageGenerationServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            server_info: Implementation {
                name: "imagen3-mcp".into(),
                version: "0.1.0".into(),
            },
            instructions: Some(r#"
Use the generate_image tool to create images from text descriptions. The returned URL can be used in markdown format like ![description](URL) to display the image.

Before generating an image, please read the <Imagen_prompt_guide> section to understand how to create effective prompts.

<Imagen_prompt_guide>
## Prompt writing basics
Description of the image to generate. Maximum prompt length is 480 tokens. A good prompt is descriptive and clear, and makes use of meaningful keywords and modifiers. Start by thinking of your subject, context, and style.
Example Prompt: A sketch (style) of a modern apartment building (subject) surrounded by skyscrapers (context and background).
1. Subject: The first thing to think about with any prompt is the subject: the object, person, animal, or scenery you want an image of.
2. Context and background: Just as important is the background or context in which the subject will be placed. Try placing your subject in a variety of backgrounds. For example, a studio with a white background, outdoors, or indoor environments.
3. Style: Finally, add the style of image you want. Styles can be general (painting, photograph, sketches) or very specific (pastel painting, charcoal drawing, isometric 3D). You can also combine styles.
After you write a first version of your prompt, refine your prompt by adding more details until you get to the image that you want. Iteration is important. Start by establishing your core idea, and then refine and expand upon that core idea until the generated image is close to your vision.
Imagen 3 can transform your ideas into detailed images, whether your prompts are short or long and detailed. Refine your vision through iterative prompting, adding details until you achieve the perfect result.
Example Prompt: close-up photo of a woman in her 20s, street photography, movie still, muted orange warm tones
Example Prompt: captivating photo of a woman in her 20s utilizing a street photography style. The image should look like a movie still with muted orange warm tones.
Additional advice for Imagen prompt writing:
- Use descriptive language: Employ detailed adjectives and adverbs to paint a clear picture for Imagen 3.
- Provide context: If necessary, include background information to aid the AI's understanding.
- Reference specific artists or styles: If you have a particular aesthetic in mind, referencing specific artists or art movements can be helpful.
- Use prompt engineering tools: Consider exploring prompt engineering tools or resources to help you refine your prompts and achieve optimal results.
- Enhancing the facial details in your personal and group images: Specify facial details as a focus of the photo (for example, use the word "portrait" in the prompt).
## Generate text in images
Imagen can add text into images, opening up more creative image generation possibilities. Use the following guidance to get the most out of this feature:
- Iterate with confidence: You might have to regenerate images until you achieve the look you want. Imagen's text integration is still evolving, and sometimes multiple attempts yield the best results.
- Keep it short: Limit text to 25 characters or less for optimal generation.
- Multiple phrases: Experiment with two or three distinct phrases to provide additional information. Avoid exceeding three phrases for cleaner compositions.
Example Prompt: A poster with the text "Summerland" in bold font as a title, underneath this text is the slogan "Summer never felt so good"
- Guide Placement: While Imagen can attempt to position text as directed, expect occasional variations. This feature is continually improving.
- Inspire font style: Specify a general font style to subtly influence Imagen's choices. Don't rely on precise font replication, but expect creative interpretations.
- Font size: Specify a font size or a general indication of size (for example, small, medium, large) to influence the font size generation.
## Advanced prompt writing techniques
Use the following examples to create more specific prompts based on attributes like photography descriptors, shapes and materials, historical art movements, and image quality modifiers.
### Photography
- Prompt includes: "A photo of..."
To use this style, start with using keywords that clearly tell Imagen that you're looking for a photograph. Start your prompts with "A photo of. . .". For example:
Example Prompt: A photo of coffee beans in a kitchen on a wooden surface
Example Prompt: A photo of a chocolate bar on a kitchen counter
Example Prompt: A photo of a modern building with water in the background
#### Photography modifiers
In the following examples, you can see several photography-specific modifiers and parameters. You can combine multiple modifiers for more precise control.
1. Camera Proximity - Close up, taken from far away
   Example Prompt: A close-up photo of coffee beans
   Example Prompt: A zoomed out photo of a small bag of coffee beans in a messy kitchen
2. Camera Position - aerial, from below
   Example Prompt: aerial photo of urban city with skyscrapers
   Example Prompt: A photo of a forest canopy with blue skies from below
3. Lighting - natural, dramatic, warm, cold
   Example Prompt: studio photo of a modern arm chair, natural lighting
   Example Prompt: studio photo of a modern arm chair, dramatic lighting
4. Camera Settings - motion blur, soft focus, bokeh, portrait
   Example Prompt: photo of a city with skyscrapers from the inside of a car with motion blur
   Example Prompt: soft focus photograph of a bridge in an urban city at night
5. Lens types - 35mm, 50mm, fisheye, wide angle, macro
   Example Prompt: photo of a leaf, macro lens
   Example Prompt: street photography, new york city, fisheye lens
6. Film types - black and white, polaroid
   Example Prompt: a polaroid portrait of a dog wearing sunglasses
   Example Prompt: black and white photo of a dog wearing sunglasses
### Illustration and art
- Prompt includes: "A painting of...", "A sketch of..."
Art styles vary from monochrome styles like pencil sketches, to hyper-realistic digital art. For example, the following images use the same prompt with different styles:
"An [art style or creation technique] of an angular sporty electric sedan with skyscrapers in the background"
Example Prompt: A technical pencil drawing of an angular...
Example Prompt: A charcoal drawing of an angular...
Example Prompt: A color pencil drawing of an angular...
Example Prompt: A pastel painting of an angular...
Example Prompt: A digital art of an angular...
Example Prompt: An art deco (poster) of an angular...
#### Shapes and materials
- Prompt includes: "...made of...", "...in the shape of..."
One of the strengths of this technology is that you can create imagery that is otherwise difficult or impossible. For example, you can recreate your company logo in different materials and textures.
Example Prompt: a duffle bag made of cheese
Example Prompt: neon tubes in the shape of a bird
Example Prompt: an armchair made of paper, studio photo, origami style
#### Historical art references
- Prompt includes: "...in the style of..."
Certain styles have become iconic over the years. The following are some ideas of historical painting or art styles that you can try.
"generate an image in the style of [art period or movement] : a wind farm"
Example Prompt: generate an image in the style of an impressionist painting: a wind farm
Example Prompt: generate an image in the style of a renaissance painting: a wind farm
Example Prompt: generate an image in the style of pop art: a wind farm
### Image quality modifiers
Certain keywords can let the model know that you're looking for a high-quality asset. Examples of quality modifiers include the following:
- General Modifiers - high-quality, beautiful, stylized
- Photos - 4K, HDR, Studio Photo
- Art, Illustration - by a professional, detailed
The following are a few examples of prompts without quality modifiers and the same prompt with quality modifiers.
Example Prompt: (no quality modifiers): a photo of a corn stalk
Example Prompt: (with quality modifiers): 4k HDR beautiful photo of a corn stalk taken by a professional photographer
### Aspect ratios
Imagen 3 image generation lets you set five distinct image aspect ratios.
1. Square (1:1, default) - A standard square photo. Common uses for this aspect ratio include social media posts.
2. Fullscreen (4:3) - This aspect ratio is commonly used in media or film. It is also the dimensions of most old (non-widescreen) TVs and medium format cameras. It captures more of the scene horizontally (compared to 1:1), making it a preferred aspect ratio for photography.
   Example Prompt: close up of a musician's fingers playing the piano, black and white film, vintage (4:3 aspect ratio)
   Example Prompt: A professional studio photo of french fries for a high end restaurant, in the style of a food magazine (4:3 aspect ratio)
3. Portrait full screen (3:4) - This is the fullscreen aspect ratio rotated 90 degrees. This lets to capture more of the scene vertically compared to the 1:1 aspect ratio.
   Example Prompt: a woman hiking, close of her boots reflected in a puddle, large mountains in the background, in the style of an advertisement, dramatic angles (3:4 aspect ratio)
   Example Prompt: aerial shot of a river flowing up a mystical valley (3:4 aspect ratio)
4. Widescreen (16:9) - This ratio has replaced 4:3 and is now the most common aspect ratio for TVs, monitors, and mobile phone screens (landscape). Use this aspect ratio when you want to capture more of the background (for example, scenic landscapes).
   Example Prompt: a man wearing all white clothing sitting on the beach, close up, golden hour lighting (16:9 aspect ratio)
5. Portrait (9:16) - This ratio is widescreen but rotated. This a relatively new aspect ratio that has been popularized by short form video apps (for example, YouTube shorts). Use this for tall objects with strong vertical orientations such as buildings, trees, waterfalls, or other similar objects.
   Example Prompt: a digital render of a massive skyscraper, modern, grand, epic with a beautiful sunset in the background (9:16 aspect ratio)
### Photorealistic images
Different versions of the image generation model might offer a mix of artistic and photorealistic output. Use the following wording in prompts to generate more photorealistic output, based on the subject you want to generate.
Note: Take these keywords as general guidance when you try to create photorealistic images. They aren't required to achieve your goal.
| Use case | Lens type | Focal lengths | Additional details |
| --- | --- | --- | --- |
| People (portraits) | Prime, zoom | 24-35mm | black and white film, Film noir, Depth of field, duotone (mention two colors) |
| Food, insects, plants (objects, still life) | Macro | 60-105mm | High detail, precise focusing, controlled lighting |
| Sports, wildlife (motion) | Telephoto zoom | 100-400mm | Fast shutter speed, Action or movement tracking |
| Astronomical, landscape (wide-angle) | Wide-angle | 10-24mm | Long exposure times, sharp focus, long exposure, smooth water or clouds |
#### Portraits
| Use case | Lens type | Focal lengths | Additional details |
| --- | --- | --- | --- |
| People (portraits) | Prime, zoom | 24-35mm | black and white film, Film noir, Depth of field, duotone (mention two colors) |
Using several keywords from the table, Imagen can generate the following portraits:
Example Prompt: A woman, 35mm portrait, blue and grey duotones
Example Prompt: A woman, 35mm portrait, film noir
#### Objects:
| Use case | Lens type | Focal lengths | Additional details |
| --- | --- | --- | --- |
| Food, insects, plants (objects, still life) | Macro | 60-105mm | High detail, precise focusing, controlled lighting |
Using several keywords from the table, Imagen can generate the following object images:
Example Prompt: leaf of a prayer plant, macro lens, 60mm
Example Prompt: a plate of pasta, 100mm Macro lens
#### Motion
| Use case | Lens type | Focal lengths | Additional details |
| --- | --- | --- | --- |
| Sports, wildlife (motion) | Telephoto zoom | 100-400mm | Fast shutter speed, Action or movement tracking |
Using several keywords from the table, Imagen can generate the following motion images:
Example Prompt: a winning touchdown, fast shutter speed, movement tracking
Example Prompt: A deer running in the forest, fast shutter speed, movement tracking
#### Wide-angle
| Use case | Lens type | Focal lengths | Additional details |
| --- | --- | --- | --- |
| Astronomical, landscape (wide-angle) | Wide-angle | 10-24mm | Long exposure times, sharp focus, long exposure, smooth water or clouds |
Using several keywords from the table, Imagen can generate the following wide-angle images:
Example Prompt: an expansive mountain range, landscape wide angle 10mm
Example Prompt: a photo of the moon, astro photography, wide angle 10mm
</Imagen_prompt_guide>
            "#.trim().into()),
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            ..Default::default()
        }
    }
}

// Create resources directory if it doesn't exist using cross-platform approach
async fn ensure_resources_dir() -> std::io::Result<PathBuf> {
    // Get application data directory in a cross-platform way
    let project_dirs = ProjectDirs::from("cn", "hamflx", "imagen3-mcp").ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Could not determine application data directory",
        )
    })?;

    // Use data_local_dir for Windows (AppData\Local), data_dir for macOS/Linux
    let base_dir = project_dirs.data_local_dir();

    // Create full path to resources directory
    let resources_path = base_dir.join("artifacts");
    let images_dir = resources_path.join("images");

    // Create directories if they don't exist
    if !resources_path.exists() {
        tokio::fs::create_dir_all(&resources_path).await?;
    }

    if !images_dir.exists() {
        tokio::fs::create_dir(&images_dir).await?;
    }

    Ok(resources_path)
}

// Handler to list images in the images directory
async fn list_images(resources_path: PathBuf) -> Result<Vec<String>, std::io::Error> {
    let images_dir = resources_path.join("images");
    let mut images = Vec::new();

    let mut entries = tokio::fs::read_dir(images_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.is_file() {
            if let Some(filename) = path.file_name() {
                if let Some(filename_str) = filename.to_str() {
                    images.push(filename_str.to_string());
                }
            }
        }
    }

    Ok(images)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Ensure resources directories exist and get the path
    let resources_path = ensure_resources_dir().await?;

    // Create service for MCP
    let service = ImageGenerationServer {
        resources_path: resources_path.clone(),
    };

    // Check if GEMINI_API_KEY is set
    if env::var("GEMINI_API_KEY").is_err() {
        eprintln!(
            "Error: GEMINI_API_KEY environment variable is not set. Image generation will fail."
        );
        std::process::exit(1);
    }

    // Set up static file server with warp
    let images_path = resources_path.join("images");
    let resources_path_clone = resources_path.clone();

    // Route for serving images
    let images_route = warp::path("images")
        .and(warp::fs::dir(images_path))
        .with(warp::cors().allow_any_origin());

    // Route for listing available images
    let list_images_route = warp::path("list-images").and_then(move || {
        let path = resources_path_clone.clone();
        async move {
            match list_images(path).await {
                Ok(images) => Ok(warp::reply::json(&images)),
                Err(_) => Err(warp::reject::not_found()),
            }
        }
    });

    // Combine all routes
    let routes = images_route.or(list_images_route);

    // Start HTTP server in a separate task
    let http_server = warp::serve(routes).run(([127, 0, 0, 1], 9981));

    let http_handle = tokio::spawn(http_server);

    // Start MCP server in the main task
    let mcp_future = ServiceExt::serve(service, (tokio::io::stdin(), tokio::io::stdout()))
        .await?
        .waiting();

    // Run MCP server to completion
    mcp_future.await?;

    // If we get here, the MCP server has shut down, so cancel the HTTP server
    http_handle.abort();

    Ok(())
}
